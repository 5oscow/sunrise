use super::*;
use mod_api::prelude::*;
use mod_api::{
    AudioRefusal, AudioStreamRef, AudioStreamSource, AudioStreamState, SoundSource, SpecFault,
    StreamPlacement, StreamSpec,
};
use std::cell::RefCell;
use std::sync::Mutex;

/// What the game's stream was asked, in order, and that it was let go.
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The game's side of one open stream: it keeps a log of the calls the guest makes.
struct Stream;
impl AudioStreamSource for Stream {
    fn push(&self, samples: &[i16]) -> u32 {
        LOG.lock().unwrap().push(format!("push {samples:?}"));
        samples.len() as u32 / 2
    }
    fn set_placement(&self, placement: &StreamPlacement) -> bool {
        LOG.lock().unwrap().push(format!("placement {placement:?}"));
        !matches!(placement, StreamPlacement::At(_))
    }
    fn set_volume(&self, volume: f32) -> bool {
        LOG.lock().unwrap().push(format!("volume {volume}"));
        true
    }
    fn state(&self) -> AudioStreamState {
        AudioStreamState::Unplaced
    }
    fn buffered_ms(&self) -> u32 {
        42
    }
    fn stop(&self) {
        LOG.lock().unwrap().push("stop".into());
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        LOG.lock().unwrap().push("dropped".into());
    }
}

#[derive(Default)]
struct Client(RefCell<Vec<StreamSpec>>);
impl mod_api::Host for Client {}
mod_api::impl_host_areas!(Client);
impl mod_api::ClientHost for Client {}
mod_api::impl_client_host_areas!(Client, except(audio));
impl AudioClientHost for Client {
    fn open_audio_stream(&self, spec: &StreamSpec) -> Result<AudioStreamRef, AudioRefusal> {
        self.0.borrow_mut().push(spec.clone());
        match spec.channels {
            1 | 2 => Ok(AudioStreamRef::new(Arc::new(Stream))),
            _ => Err(AudioRefusal::BadSpec(SpecFault::Channels)),
        }
    }
}

#[test]
fn wire_loopback_a_guest_holds_an_audio_stream_by_id_and_drops_it_to_stop_it() {
    mod_api::__private::initialize().unwrap();
    let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
    let mut client = Client::default();
    let mut frame = boundary::Frame::default();
    frame.set_client_host(&client);
    let mut cx = boundary::Cx::default();
    cx.frames.push(frame);
    let cx = Box::into_raw(Box::new(cx));
    let _guest = guest::resource_scope();
    // SAFETY: the context outlives every call; the transport is cleared before it is freed.
    guest::set_transport(Some(unsafe { host_dispatch::transport(cx) }));

    let spec = StreamSpec::new(SoundSource::Voice, 1, 24_000)
        .with_placement(StreamPlacement::At([1.5, 64.0, -3.0]))
        .with_prebuffer_ms(150);
    let stream = guest::Guest.open_audio_stream(&spec).unwrap();
    assert_eq!(client.0.borrow().as_slice(), [spec]);
    assert_eq!(stream.push(&[1, -2, 3, 4]), 2);
    assert!(!stream.set_placement(&StreamPlacement::At([0.0, 1.0, 2.0])));
    assert!(stream.set_placement(&StreamPlacement::Flat));
    assert!(stream.set_volume(0.5));
    assert_eq!(stream.state(), AudioStreamState::Unplaced);
    assert_eq!(stream.buffered_ms(), 42);
    // A refusal crosses by name.
    let refused = guest::Guest.open_audio_stream(&StreamSpec::new(SoundSource::Voice, 3, 24_000));
    assert_eq!(
        refused.err(),
        Some(AudioRefusal::BadSpec(SpecFault::Channels))
    );
    drop(stream);
    guest::set_transport(None);

    assert_eq!(
        *LOG.lock().unwrap(),
        [
            "push [1, -2, 3, 4]",
            "placement At([0.0, 1.0, 2.0])",
            "placement Flat",
            "volume 0.5",
            "dropped",
        ]
    );
    // SAFETY: the transport that held the pointer is gone.
    drop(unsafe { Box::from_raw(cx) });
}
