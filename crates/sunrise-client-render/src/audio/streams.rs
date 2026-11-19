//! Mods' audio streams (`ClientHost::open_audio_stream`) in the mixer: a pool of their own
//! (never vanilla's streaming slots), each a fixed ring a mod pushes PCM into from any thread and
//! the output callback reads, resampled to the device rate, with no allocation and no lock the
//! callback waits on.
use super::{Basis, Mixer, distance_gain};
use mod_api::{
    AUDIO_STREAM_PREBUFFER_MAX_MS, AUDIO_STREAM_PREBUFFER_MIN_MS, AUDIO_STREAM_RATE_MAX,
    AUDIO_STREAM_RATE_MIN, AUDIO_STREAM_RING_MAX_MS, AUDIO_STREAMS_PER_MOD, AUDIO_STREAMS_TOTAL,
    AudioRefusal, AudioStreamSource, AudioStreamState, EntityRef, SpecFault, StreamPlacement,
    StreamSpec,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI16, AtomicU32, AtomicU64, Ordering::*},
};

/// How far from the origin a position may lie, in blocks (the world border's limit).
const POSITION_LIMIT: f64 = 30_000_000.0;

/// Where a stream was last placed, as the main thread reads it: the callback sees only
/// `positional`, `placed` and the position.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Held {
    Flat,
    At,
    Entity(EntityRef),
    /// A dimension change took the placement away: silent until the next `set_placement`.
    Cleared,
}

/// What a stream's producer and the mixer share.
pub(super) struct StreamShared {
    ring: Box<[AtomicI16]>,
    /// Samples consumed and written since the stream opened; the ring index is the count modulo
    /// its length. Only the callback moves `head`, only the one pusher moves `tail`.
    head: AtomicU64,
    tail: AtomicU64,
    channels: usize,
    rate: u32,
    prebuffer_frames: u64,
    /// The stream's `SoundSource` index.
    category: usize,
    range: f32,
    owner: String,
    volume: AtomicU32,
    /// Prebuffered and playing, as against buffering.
    playing: AtomicBool,
    closed: AtomicBool,
    /// Heard from a place: a point or an entity.
    positional: AtomicBool,
    /// A positional stream's place is known now.
    placed: AtomicBool,
    position: [AtomicU64; 3],
    held: Mutex<Held>,
    /// Another thread is pushing: this push is refused, which keeps the ring single-producer.
    pushing: AtomicBool,
}

impl StreamShared {
    fn buffered_frames(&self) -> u64 {
        let used = self.tail.load(Acquire) - self.head.load(Acquire);
        used / self.channels as u64
    }
    fn place(&self, placement: &StreamPlacement) {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        match *placement {
            StreamPlacement::Flat => {
                self.positional.store(false, Release);
                *held = Held::Flat;
            }
            StreamPlacement::At(at) => {
                for (cell, v) in self.position.iter().zip(at) {
                    cell.store(v.to_bits(), Relaxed);
                }
                *held = Held::At;
                self.placed.store(true, Release);
                self.positional.store(true, Release);
            }
            StreamPlacement::Entity(entity) => {
                *held = Held::Entity(entity);
                self.placed.store(false, Release);
                self.positional.store(true, Release);
            }
        }
    }
}

fn check_placement(channels: u8, placement: &StreamPlacement) -> Result<(), SpecFault> {
    match placement {
        StreamPlacement::Flat => Ok(()),
        _ if channels != 1 => Err(SpecFault::StereoPositioned),
        StreamPlacement::At(at)
            if at
                .iter()
                .any(|v| !v.is_finite() || v.abs() > POSITION_LIMIT) =>
        {
            Err(SpecFault::Position)
        }
        _ => Ok(()),
    }
}

/// A mod's end of a stream: what `AudioStreamRef` calls. Dropping it stops the stream.
pub(super) struct StreamHandle(Arc<StreamShared>);

impl AudioStreamSource for StreamHandle {
    fn push(&self, samples: &[i16]) -> u32 {
        let s = &*self.0;
        if s.closed.load(Acquire) || s.pushing.swap(true, Acquire) {
            return 0;
        }
        let channels = s.channels;
        let len = s.ring.len() as u64;
        let tail = s.tail.load(Relaxed);
        let free = (len - (tail - s.head.load(Acquire))) / channels as u64;
        let frames = (samples.len() / channels).min(free as usize);
        for (i, sample) in samples[..frames * channels].iter().enumerate() {
            s.ring[((tail + i as u64) % len) as usize].store(*sample, Relaxed);
        }
        s.tail.store(tail + (frames * channels) as u64, Release);
        s.pushing.store(false, Release);
        frames as u32
    }
    fn set_placement(&self, placement: &StreamPlacement) -> bool {
        let s = &*self.0;
        if s.closed.load(Acquire) || check_placement(s.channels as u8, placement).is_err() {
            return false;
        }
        s.place(placement);
        true
    }
    fn set_volume(&self, volume: f32) -> bool {
        if self.0.closed.load(Acquire) || !(0.0..=1.0).contains(&volume) {
            return false;
        }
        self.0.volume.store(volume.to_bits(), Relaxed);
        true
    }
    fn state(&self) -> AudioStreamState {
        let s = &*self.0;
        if s.closed.load(Acquire) {
            AudioStreamState::Stopped
        } else if !s.playing.load(Acquire) {
            AudioStreamState::Buffering
        } else if s.positional.load(Acquire) && !s.placed.load(Acquire) {
            AudioStreamState::Unplaced
        } else {
            AudioStreamState::Playing
        }
    }
    fn buffered_ms(&self) -> u32 {
        (self.0.buffered_frames() * 1000 / u64::from(self.0.rate)) as u32
    }
    fn stop(&self) {
        self.0.closed.store(true, Release);
    }
}
impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The mixer's end of a stream: where the resampler is.
pub(super) struct StreamVoice {
    shared: Arc<StreamShared>,
    /// The last frame taken from the ring, and how far (0 to 1) the output is towards the next.
    current: [f32; 2],
    fraction: f64,
}

impl Mixer {
    /// Opens a stream for `owner`, a mod. The ring is allocated here, once.
    pub(super) fn open_stream(
        &mut self,
        owner: &str,
        spec: &StreamSpec,
    ) -> Result<Arc<dyn AudioStreamSource>, AudioRefusal> {
        let fault = |fault| Err(AudioRefusal::BadSpec(fault));
        if !matches!(spec.channels, 1 | 2) {
            return fault(SpecFault::Channels);
        }
        if !(AUDIO_STREAM_RATE_MIN..=AUDIO_STREAM_RATE_MAX).contains(&spec.rate) {
            return fault(SpecFault::Rate);
        }
        if !(AUDIO_STREAM_PREBUFFER_MIN_MS..=AUDIO_STREAM_PREBUFFER_MAX_MS)
            .contains(&spec.prebuffer_ms)
        {
            return fault(SpecFault::Prebuffer);
        }
        if !(0.0..=1.0).contains(&spec.volume) {
            return fault(SpecFault::Volume);
        }
        if !(spec.range.is_finite() && spec.range > 0.0) {
            return fault(SpecFault::Range);
        }
        if let Err(f) = check_placement(spec.channels, &spec.placement) {
            return fault(f);
        }
        self.streams.retain(|v| !v.shared.closed.load(Acquire));
        if self.streams.len() >= AUDIO_STREAMS_TOTAL as usize {
            return Err(AudioRefusal::TooManyStreams);
        }
        if self
            .streams
            .iter()
            .filter(|v| v.shared.owner == owner)
            .count()
            >= AUDIO_STREAMS_PER_MOD as usize
        {
            return Err(AudioRefusal::ModStreamLimit);
        }
        let channels = spec.channels as usize;
        let ring_ms = (2 * spec.prebuffer_ms + 100).min(AUDIO_STREAM_RING_MAX_MS);
        let ring_frames = (u64::from(spec.rate) * u64::from(ring_ms) / 1000) as usize;
        let shared = Arc::new(StreamShared {
            ring: (0..ring_frames * channels)
                .map(|_| AtomicI16::new(0))
                .collect(),
            head: AtomicU64::new(0),
            tail: AtomicU64::new(0),
            channels,
            rate: spec.rate,
            prebuffer_frames: u64::from(spec.rate) * u64::from(spec.prebuffer_ms) / 1000,
            category: spec.category.index(),
            range: spec.range,
            owner: owner.to_owned(),
            volume: AtomicU32::new(spec.volume.to_bits()),
            playing: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            positional: AtomicBool::new(false),
            placed: AtomicBool::new(false),
            position: Default::default(),
            held: Mutex::new(Held::Flat),
            pushing: AtomicBool::new(false),
        });
        shared.place(&spec.placement);
        if self.streams.capacity() == 0 {
            self.streams.reserve_exact(AUDIO_STREAMS_TOTAL as usize);
        }
        self.streams.push(StreamVoice {
            shared: shared.clone(),
            current: [0.0; 2],
            fraction: 0.0,
        });
        Ok(Arc::new(StreamHandle(shared)))
    }

    /// Drops the streams a mod stopped or let go of.
    pub(super) fn reap_streams(&mut self) {
        self.streams.retain(|v| !v.shared.closed.load(Acquire));
    }

    /// Follows each entity-placed stream's entity: where `resolve` finds it the stream is placed
    /// there, where it doesn't (untracked, unloaded, removed) the stream is unplaced.
    pub(super) fn follow_stream_entities(
        &mut self,
        mut resolve: impl FnMut(EntityRef) -> Option<[f64; 3]>,
    ) {
        for v in &self.streams {
            let s = &*v.shared;
            let held = *s.held.lock().unwrap_or_else(|e| e.into_inner());
            let Held::Entity(entity) = held else {
                continue;
            };
            match resolve(entity).filter(|at| at.iter().all(|c| c.is_finite())) {
                Some(at) => {
                    for (cell, c) in s.position.iter().zip(at) {
                        cell.store(c.to_bits(), Relaxed);
                    }
                    s.placed.store(true, Release);
                }
                None => s.placed.store(false, Release),
            }
        }
    }

    /// The listener left its dimension: every positional stream is silent until its mod places it
    /// again.
    pub(super) fn unplace_streams(&mut self) {
        for v in &self.streams {
            let s = &*v.shared;
            let mut held = s.held.lock().unwrap_or_else(|e| e.into_inner());
            if *held != Held::Flat {
                *held = Held::Cleared;
                s.placed.store(false, Release);
            }
        }
    }
}

/// Mixes every stream into `out` (the muffle's bus or the output), `channels` wide at `rate`.
/// Allocates nothing: the rings, the voices and the cameras were all made before.
pub(super) fn mix(
    streams: &mut [StreamVoice],
    cameras: &[([f64; 3], Basis, u32)],
    category_gain: &[f32; 11],
    out: &mut [f32],
    channels: usize,
    rate: u32,
) {
    let shared_listeners = cameras.len() > 1;
    for voice in streams.iter_mut() {
        let s = &*voice.shared;
        if s.closed.load(Acquire) {
            continue;
        }
        let ch = s.channels;
        let head = s.head.load(Relaxed);
        let tail = s.tail.load(Acquire);
        let mut avail = (tail - head) / ch as u64;
        let mut taken = 0u64;
        let len = s.ring.len() as u64;
        let frame_at = |frame: u64| -> [f32; 2] {
            let base = head + frame * ch as u64;
            let sample =
                |i: u64| f32::from(s.ring[((base + i) % len) as usize].load(Relaxed)) / 32768.0;
            let left = sample(0);
            [left, if ch > 1 { sample(1) } else { left }]
        };
        if !s.playing.load(Relaxed) {
            if avail < s.prebuffer_frames.max(2) {
                continue;
            }
            voice.current = frame_at(0);
            voice.fraction = 0.0;
            taken = 1;
            avail -= 1;
            s.playing.store(true, Release);
        }
        let volume = f32::from_bits(s.volume.load(Relaxed)) * category_gain[s.category];
        // Gain per output channel; NaN anywhere is silence.
        let [left, right] = if !s.positional.load(Relaxed) {
            if ch > 1 {
                [volume, volume]
            } else {
                [volume; 2]
            }
        } else if !s.placed.load(Acquire) {
            [0.0; 2]
        } else {
            let at = [0, 1, 2].map(|i| f64::from_bits(s.position[i].load(Relaxed)));
            let away =
                |c: &([f64; 3], Basis, u32)| [at[0] - c.0[0], at[1] - c.0[1], at[2] - c.0[2]];
            let sqr = |d: [f64; 3]| d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            match cameras
                .iter()
                .min_by(|a, b| sqr(away(a)).total_cmp(&sqr(away(b))))
            {
                None => [0.0; 2],
                Some(camera) => {
                    let d = away(camera);
                    let (forward, up, right) = camera.1;
                    let dot =
                        |a: [f64; 3], b: [f64; 3]| (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) as f32;
                    let distance = sqr(d).sqrt() as f32;
                    let gain = volume * distance_gain(Some(s.range), distance);
                    let [mut l, mut r] = super::stereo_gains_with_length(
                        dot(d, right),
                        dot(d, up),
                        dot(d, forward),
                        distance,
                    );
                    if shared_listeners {
                        let middle = (l + r) * 0.5;
                        l = middle + (l - middle) * 0.5;
                        r = middle + (r - middle) * 0.5;
                    }
                    [l * gain, r * gain]
                }
            }
        };
        let [left, right] = [left, right].map(|g| if g.is_finite() { g } else { 0.0 });
        let step = f64::from(s.rate) / f64::from(rate);
        for frame in out.chunks_mut(channels) {
            if avail == 0 {
                // Underrun: silence, and buffering again until `prebuffer_ms` is back.
                s.playing.store(false, Release);
                break;
            }
            let next = frame_at(taken);
            let t = voice.fraction as f32;
            let l = voice.current[0] + (next[0] - voice.current[0]) * t;
            let r = voice.current[1] + (next[1] - voice.current[1]) * t;
            if channels >= 2 {
                frame[0] += l * left;
                frame[1] += r * right;
            } else {
                // A mono output has no panning: the stream's own gain, the stereo pair summed.
                frame[0] += if ch > 1 {
                    (l + r) * 0.5 * left
                } else {
                    l * left
                };
            }
            voice.fraction += step;
            while voice.fraction >= 1.0 && avail > 0 {
                voice.current = frame_at(taken);
                taken += 1;
                avail -= 1;
                voice.fraction -= 1.0;
            }
        }
        s.head.store(head + taken * ch as u64, Release);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Mixer, mix as mix_output};
    use mod_api::{
        AudioRefusal, AudioStreamSource, AudioStreamState as State, SoundSource, SpecFault,
        StreamPlacement, StreamSpec,
    };
    use std::sync::Arc;

    /// A mixer with every group at full volume and the listener at the origin, facing +z.
    fn mixer() -> Mixer {
        Mixer {
            stream_gain: [1.0; 11],
            ..Mixer::default()
        }
    }
    /// A mono 48 kHz stream with the shortest prebuffer: 960 frames to start, a 6720 frame ring.
    fn spec() -> StreamSpec {
        StreamSpec::new(SoundSource::Voice, 1, 48_000).with_prebuffer_ms(20)
    }
    /// The next `frames` of stereo output at 48 kHz.
    fn play(m: &mut Mixer, frames: usize) -> Vec<f32> {
        let mut data = vec![0.0; frames * 2];
        mix_output(m, &mut data, 2, 48_000);
        data
    }
    /// Half-scale samples: 0.5 once mixed at gain 1.
    fn half(frames: usize) -> Vec<i16> {
        vec![16_384; frames]
    }

    #[test]
    fn a_stream_is_silent_until_its_prebuffer_is_in_and_then_plays() {
        let mut m = mixer();
        let stream = m.open_stream("mod", &spec()).unwrap();
        assert_eq!(stream.push(&half(959)), 959);
        assert!(play(&mut m, 8).iter().all(|s| *s == 0.0));
        assert_eq!(stream.state(), State::Buffering);
        assert_eq!(stream.push(&half(1)), 1);
        let out = play(&mut m, 8);
        assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6), "{out:?}");
        assert_eq!(stream.state(), State::Playing);
    }

    #[test]
    fn an_underrun_plays_silence_and_goes_back_to_buffering() {
        let mut m = mixer();
        let stream = m.open_stream("mod", &spec()).unwrap();
        stream.push(&half(960));
        let out = play(&mut m, 2_000);
        assert!(out[..1_800].iter().all(|s| *s > 0.4));
        assert!(out[3_000..].iter().all(|s| *s == 0.0));
        assert_eq!(stream.state(), State::Buffering);
        // It waits for a whole prebuffer again, and then plays on: it never ended.
        stream.push(&half(500));
        assert!(play(&mut m, 8).iter().all(|s| *s == 0.0));
        stream.push(&half(500));
        assert!(play(&mut m, 8).iter().all(|s| *s > 0.4));
    }

    #[test]
    fn a_full_ring_refuses_the_excess() {
        let mut m = mixer();
        let stream = m.open_stream("mod", &spec()).unwrap();
        assert_eq!(stream.push(&half(10_000)), 6_720);
        assert_eq!(stream.push(&half(1)), 0);
        assert_eq!(stream.buffered_ms(), 140);
        // Playing makes room again: 480 frames played, and the one the resampler holds.
        play(&mut m, 480);
        assert_eq!(stream.push(&half(10_000)), 481);
    }

    #[test]
    fn a_mono_stream_at_half_its_range_has_half_the_gain() {
        let mut m = mixer();
        let range = 16.0;
        let heard = |m: &mut Mixer, at: [f64; 3]| {
            let stream = m
                .open_stream(
                    "mod",
                    &spec()
                        .with_placement(StreamPlacement::At(at))
                        .with_range(range),
                )
                .unwrap();
            stream.push(&half(2_000));
            let out = play(m, 16);
            stream.stop();
            m.reap_streams();
            out[8]
        };
        let here = heard(&mut m, [0.0, 0.0, 0.0]);
        let half_way = heard(&mut m, [0.0, 0.0, f64::from(range) / 2.0]);
        let edge = heard(&mut m, [0.0, 0.0, f64::from(range)]);
        assert!(here > 0.2);
        assert!((half_way / here - 0.5).abs() < 1e-3, "{half_way} {here}");
        assert_eq!(edge, 0.0);
    }

    #[test]
    fn a_stereo_stream_with_a_placement_is_refused() {
        let mut m = mixer();
        let stereo = StreamSpec::new(SoundSource::Voice, 2, 48_000);
        let at = StreamPlacement::At([1.0, 2.0, 3.0]);
        assert!(matches!(
            m.open_stream("mod", &stereo.clone().with_placement(at)),
            Err(AudioRefusal::BadSpec(SpecFault::StereoPositioned))
        ));
        let flat = m.open_stream("mod", &stereo).unwrap();
        assert!(!flat.set_placement(&at));
        assert!(flat.set_placement(&StreamPlacement::Flat));
    }

    #[test]
    fn streams_are_capped_per_mod_and_overall() {
        let mut m = mixer();
        let mut open = Vec::new();
        for _ in 0..32 {
            open.push(m.open_stream("a", &spec()).unwrap());
        }
        assert!(matches!(
            m.open_stream("a", &spec()),
            Err(AudioRefusal::ModStreamLimit)
        ));
        for _ in 0..32 {
            open.push(m.open_stream("b", &spec()).unwrap());
        }
        assert!(matches!(
            m.open_stream("c", &spec()),
            Err(AudioRefusal::TooManyStreams)
        ));
        // A stopped stream frees its place.
        open.pop().unwrap().stop();
        assert!(m.open_stream("c", &spec()).is_ok());
    }

    #[test]
    fn bad_numbers_are_refused_and_never_reach_the_mixer() {
        let mut m = mixer();
        let bad = |m: &mut Mixer, s: StreamSpec| m.open_stream("mod", &s).err();
        assert_eq!(
            bad(&mut m, StreamSpec::new(SoundSource::Voice, 3, 48_000)),
            Some(AudioRefusal::BadSpec(SpecFault::Channels))
        );
        assert_eq!(
            bad(&mut m, StreamSpec::new(SoundSource::Voice, 1, 7_999)),
            Some(AudioRefusal::BadSpec(SpecFault::Rate))
        );
        assert_eq!(
            bad(&mut m, spec().with_volume(f32::NAN)),
            Some(AudioRefusal::BadSpec(SpecFault::Volume))
        );
        assert_eq!(
            bad(&mut m, spec().with_range(f32::INFINITY)),
            Some(AudioRefusal::BadSpec(SpecFault::Range))
        );
        assert_eq!(
            bad(
                &mut m,
                spec().with_placement(StreamPlacement::At([f64::NAN, 0.0, 0.0]))
            ),
            Some(AudioRefusal::BadSpec(SpecFault::Position))
        );
        let stream = m.open_stream("mod", &spec()).unwrap();
        assert!(!stream.set_volume(f32::NAN));
        assert!(!stream.set_placement(&StreamPlacement::At([0.0, f64::INFINITY, 0.0])));
        assert!(m.streams.len() == 1);
    }

    #[test]
    fn an_unplaced_stream_is_silent_but_its_audio_is_still_consumed() {
        let mut m = mixer();
        let entity = mod_api::EntityRef::new(mod_api::WorldEpoch::new(1), 7);
        let stream = m
            .open_stream(
                "mod",
                &spec().with_placement(StreamPlacement::Entity(entity)),
            )
            .unwrap();
        stream.push(&half(2_000));
        m.follow_stream_entities(|_| None);
        let before = stream.buffered_ms();
        assert!(play(&mut m, 480).iter().all(|s| *s == 0.0));
        assert_eq!(stream.state(), State::Unplaced);
        assert_eq!(before - stream.buffered_ms(), 10);
        // Tracked again, at a place a NaN spoils: still unplaced; at a real one: heard.
        m.follow_stream_entities(|_| Some([f64::NAN, 0.0, 0.0]));
        assert_eq!(stream.state(), State::Unplaced);
        m.follow_stream_entities(|e| (e == entity).then_some([0.0, 0.0, 1.0]));
        assert!(play(&mut m, 16).iter().any(|s| *s > 0.0));
        assert_eq!(stream.state(), State::Playing);
    }

    #[test]
    fn a_dimension_change_unplaces_positional_streams_until_they_are_placed_again() {
        let mut m = mixer();
        let at = StreamPlacement::At([0.0, 0.0, 1.0]);
        let positional = m.open_stream("mod", &spec().with_placement(at)).unwrap();
        let flat = m.open_stream("mod", &spec()).unwrap();
        for stream in [&positional, &flat] {
            stream.push(&half(2_000));
        }
        play(&mut m, 8);
        m.unplace_streams();
        assert_eq!(positional.state(), State::Unplaced);
        assert_eq!(flat.state(), State::Playing);
        assert!(positional.set_placement(&at));
        assert_eq!(positional.state(), State::Playing);
    }

    #[test]
    fn dropping_the_handle_stops_the_stream_and_the_pool_lets_it_go() {
        let mut m = mixer();
        let stream: Arc<dyn AudioStreamSource> = m.open_stream("mod", &spec()).unwrap();
        let handle = mod_api::AudioStreamRef::new(stream);
        drop(handle);
        m.reap_streams();
        assert!(m.streams.is_empty());
    }

    #[test]
    fn a_stream_is_resampled_to_the_output_rate() {
        let mut m = mixer();
        let stream = m
            .open_stream(
                "mod",
                &StreamSpec::new(SoundSource::Voice, 1, 24_000).with_prebuffer_ms(20),
            )
            .unwrap();
        stream.push(&half(960));
        // 960 output frames at 48 kHz are 480 frames of a 24 kHz stream, 20 ms, and the
        // resampler holds one more: 479 of the 960 queued are left, a hair under 20 ms.
        play(&mut m, 960);
        assert_eq!(stream.buffered_ms(), 19);
    }

    #[test]
    fn the_muffle_turns_streams_down_with_everything_else() {
        let mut m = mixer();
        let stream = m.open_stream("mod", &spec()).unwrap();
        stream.push(&half(2_000));
        m.muffle.target = [1.0, 0.0];
        m.muffle.current = [1.0, 0.0];
        assert!(play(&mut m, 64).iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn a_pushing_thread_never_waits_on_the_mixer() {
        let mut m = mixer();
        let stream = m.open_stream("mod", &spec()).unwrap();
        let pusher = std::thread::spawn(move || stream.push(&half(100)));
        // The mixer is locked meanwhile in the game; the push goes through regardless.
        assert_eq!(pusher.join().unwrap(), 100);
    }
}
