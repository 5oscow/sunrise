//! The server's check of a player a moving collider carries.
//!
//! A client moves its player with the collider as it last heard of it, which is the collider's
//! transform from some ticks ago: the round trip, plus how stale the client's copy was. The
//! server cannot judge such a position against where the collider is now, so it keeps each
//! collider's recent transforms in a [`FrameLog`] and asks, for a report, whether it fits the
//! collider as it was at some age inside a window around the age it expects.
//!
//! A report fits an age when, in the collider's frame at that age, the player stands on the top
//! of a box (within [`TOP_TOLERANCE`]), or is above one, is over the box, and is not inside any
//! box deeper than the tolerance. Of the ages that fit, the one nearest the age the server
//! expects wins, so the age cannot wander out of the window one honest report at a time. The
//! move itself, in the collider's frame, is then judged by the ordinary move check of the
//! engine against the colliders as they were at that age, so a carried player is held to
//! exactly what a walking one is.
//!
//! Everything here is pure geometry and bookkeeping: no engine types.
use super::{Aabb, Vec3, oriented::ColliderBoxes, oriented::Frame};
use std::collections::VecDeque;

/// How far from the top of a box the feet may be and still stand on it, in blocks.
pub const TOP_TOLERANCE: f64 = 0.05;
/// How far past a box's edge the feet may be and still be over it, in blocks.
pub const EDGE_MARGIN: f64 = 0.3;
/// The ages tried either side of the expected one, in ticks.
pub const WINDOW: f64 = 4.0;
/// The most round-trip time the server allows for, in ticks (one second). A player further away
/// than that is judged as if it were this far: its reports run past the window and are
/// corrected.
pub const MAX_ROUND_TRIP_TICKS: f64 = 20.0;
/// What the client's own interpolation adds to the age in steady state, in ticks.
pub const LAG_TICKS: f64 = 2.0;
/// How many ticks of a collider's history the server keeps: the oldest age the window can ask.
pub const KEEP_TICKS: u64 = (MAX_ROUND_TRIP_TICKS + LAG_TICKS + WINDOW) as u64 + 1;
/// The ages are searched in steps of this many ticks.
const STEP: f64 = 1.0 / 16.0;

/// One collider's frames, newest last. A tick on which the collider did not move stores
/// nothing, so a collider that stands still costs one entry however long it does.
#[derive(Clone, Debug, Default)]
pub struct FrameLog {
    entries: VecDeque<(u64, Frame)>,
}

impl FrameLog {
    /// Records the collider's frame at `tick`, which must not be earlier than the last one
    /// recorded, and forgets what is older than [`KEEP_TICKS`]. Whether the frame is a change.
    pub fn record(&mut self, tick: u64, frame: Frame) -> bool {
        if let Some(&(last, held)) = self.entries.back() {
            if held == frame {
                return false;
            }
            // The tick before this change still had the old frame: keep it, so the stretch
            // between two moves is flat and the interpolation never runs across it.
            if last + 1 < tick {
                self.entries.push_back((tick - 1, held));
            }
        }
        self.entries.push_back((tick, frame));
        while self
            .entries
            .get(1)
            .is_some_and(|&(next, _)| next + KEEP_TICKS <= tick)
        {
            self.entries.pop_front();
        }
        true
    }

    /// The tick of the newest frame (not of the last change: see [`Self::frame_at`]).
    fn newest(&self) -> Option<&(u64, Frame)> {
        self.entries.back()
    }

    /// The frame `age` ticks before `now`, between two recorded ticks interpolated. An age
    /// older than the log reaches gives its oldest frame, which is where the collider stood
    /// all along. `None` for a collider never recorded.
    pub fn frame_at(&self, now: u64, age: f64) -> Option<Frame> {
        let &(last_tick, last) = self.newest()?;
        let at = now as f64 - age.max(0.0);
        if at >= last_tick as f64 {
            return Some(last);
        }
        let first = self.entries.front()?;
        if at <= first.0 as f64 {
            return Some(first.1);
        }
        let after = self.entries.partition_point(|&(tick, _)| tick as f64 <= at);
        let (t0, f0) = self.entries[after - 1];
        let (t1, f1) = self.entries[after];
        Some(f0.interpolate(&f1, (at - t0 as f64) / (t1 - t0) as f64))
    }

    /// The box holding every place `local` (a box in the collider's frame) has been in over the
    /// log, for finding the colliders a position can have stood on.
    pub fn swept(&self, local: Aabb) -> Option<Aabb> {
        let mut all: Option<Aabb> = None;
        for (_, frame) in &self.entries {
            let at = frame.world_bounds(local);
            all = Some(all.map_or(at, |a| a.union_bounds(at)));
        }
        all
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The body the check is about, as the move check sees it.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub half_width: f64,
    pub height: f64,
}

/// How a position stands to a collider's boxes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Footing {
    /// On the top of a box, within [`TOP_TOLERANCE`].
    Standing,
    /// Over a box and higher than its top.
    Above,
}

/// How the feet at `local` (in the collider's frame) stand to `boxes`: `None` when the body is
/// inside a box deeper than [`TOP_TOLERANCE`], or not over any box at or above its top.
pub fn footing(boxes: &ColliderBoxes, local: Vec3, body: Body) -> Option<Footing> {
    let reach = Vec3::new(EDGE_MARGIN + 0.05, TOP_TOLERANCE + 0.05, EDGE_MARGIN + 0.05);
    let around = Aabb::new(
        Vec3::new(
            local.x - body.half_width,
            local.y,
            local.z - body.half_width,
        ),
        Vec3::new(
            local.x + body.half_width,
            local.y + body.height,
            local.z + body.half_width,
        ),
    );
    let mut near = Vec::new();
    boxes.candidates(around.inflate(reach), &mut near);
    let mut result = None;
    for &index in &near {
        let b = boxes.boxes()[index as usize];
        // Inside: the body's box meets the box below its top by more than the tolerance.
        let below_top = Aabb::new(b.min, Vec3::new(b.max.x, b.max.y - TOP_TOLERANCE, b.max.z));
        if around.intersects(below_top) {
            return None;
        }
        let over = local.x >= b.min.x - EDGE_MARGIN
            && local.x <= b.max.x + EDGE_MARGIN
            && local.z >= b.min.z - EDGE_MARGIN
            && local.z <= b.max.z + EDGE_MARGIN;
        if !over {
            continue;
        }
        let height = local.y - b.max.y;
        if height.abs() <= TOP_TOLERANCE {
            result = Some(Footing::Standing);
        } else if height > TOP_TOLERANCE && result.is_none() {
            result = Some(Footing::Above);
        }
    }
    result
}

/// The ages the server accepts for a report, around the one it expects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ages {
    /// The first age tried, inside the window.
    pub anchor: f64,
    pub lo: f64,
    pub hi: f64,
}

impl Ages {
    /// The window for a connection with this round-trip time (milliseconds, the smoothed
    /// keepalive time) and, if the client sent one, how many ticks stale its copy of the
    /// collider was when it moved. The age to expect is the round trip plus that, else plus
    /// [`LAG_TICKS`]; the hint is only where the search starts, clamped into the window.
    pub fn new(round_trip_ms: i32, hint_ticks: Option<u8>) -> Self {
        let trip = (f64::from(round_trip_ms.max(0)) / 50.0).min(MAX_ROUND_TRIP_TICKS);
        let expected = trip + LAG_TICKS;
        let (lo, hi) = ((expected - WINDOW).max(0.0), expected + WINDOW);
        let anchor = hint_ticks.map_or(expected, |h| trip + f64::from(h));
        Self {
            anchor: anchor.clamp(lo, hi),
            lo,
            hi,
        }
    }
}

/// A report that fits.
#[derive(Clone, Copy, Debug)]
pub struct Fit {
    /// The age it fits, in ticks before now.
    pub age: f64,
    /// The collider's frame at that age.
    pub frame: Frame,
    /// The feet in that frame.
    pub local: Vec3,
    pub footing: Footing,
}

/// The age nearest the anchor at which the feet at world position `to` fit `boxes` as `log`
/// has them. With `standing_only`, only a position on a box top fits: how a player first
/// attaches. `None` when no age in the window fits.
pub fn judge(
    log: &FrameLog,
    now: u64,
    boxes: &ColliderBoxes,
    to: Vec3,
    body: Body,
    ages: Ages,
    standing_only: bool,
) -> Option<Fit> {
    let fits = |age: f64| {
        let frame = log.frame_at(now, age)?;
        let local = frame.to_local(to);
        let footing = footing(boxes, local, body)?;
        (!standing_only || footing == Footing::Standing).then_some(Fit {
            age,
            frame,
            local,
            footing,
        })
    };
    let steps = ((ages.hi - ages.lo) / STEP).ceil() as usize;
    for j in 0..=steps {
        let d = j as f64 * STEP;
        for age in [ages.anchor - d, ages.anchor + d] {
            if (ages.lo..=ages.hi).contains(&age)
                && let Some(fit) = fits(age)
            {
                return Some(fit);
            }
        }
    }
    None
}

/// What the server holds of a player a collider carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carried {
    /// The collider's entity.
    pub collider: u32,
    /// The age the last accepted report fitted.
    pub age: f64,
    /// The feet at the last accepted report, in the collider's frame.
    pub local: Vec3,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck() -> ColliderBoxes {
        ColliderBoxes::new(vec![Aabb::new(
            Vec3::new(-1.5, -1.0, -10.0),
            Vec3::new(1.5, 0.0, 10.0),
        )])
    }

    const BODY: Body = Body {
        half_width: 0.3,
        height: 1.8,
    };

    /// A collider moving along z at `speed` blocks a tick, logged for ticks 0..=`now`.
    fn train(now: u64, speed: f64) -> FrameLog {
        let mut log = FrameLog::default();
        for t in 0..=now {
            log.record(
                t,
                Frame::translation(Vec3::new(0.0, 64.0, speed * t as f64)),
            );
        }
        log
    }

    #[test]
    fn a_log_answers_between_its_ticks_and_beyond_its_ends() {
        let log = train(30, 2.0);
        let z = |age: f64| log.frame_at(30, age).unwrap().origin.z;
        assert_eq!(z(0.0), 60.0);
        assert_eq!(z(1.0), 58.0);
        assert!((z(2.5) - 55.0).abs() < 1e-9);
        // Past what it keeps it answers its oldest frame, not a guess.
        let oldest = z(40.0);
        assert!(oldest < z(20.0));
        assert_eq!(z(40.0), z(100.0));
    }

    #[test]
    fn a_collider_that_stands_still_costs_one_entry_and_moves_cleanly_when_it_starts() {
        let mut log = FrameLog::default();
        let at = |z: f64| Frame::translation(Vec3::new(0.0, 0.0, z));
        for t in 0..100 {
            log.record(t, at(0.0));
        }
        assert_eq!(log.entries.len(), 1);
        log.record(100, at(2.0));
        // The tick before the move still has the old frame, and the move interpolates only
        // across the one tick it took.
        assert_eq!(log.frame_at(100, 1.0).unwrap().origin.z, 0.0);
        assert!((log.frame_at(100, 0.5).unwrap().origin.z - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_log_keeps_the_ticks_the_window_can_ask_for_and_no_more() {
        let log = train(1000, 1.0);
        assert!(
            log.entries.len() as u64 <= KEEP_TICKS + 1,
            "{}",
            log.entries.len()
        );
        // The oldest frame it answers is no older than the window can reach.
        let oldest = log.frame_at(1000, 1.0e9).unwrap().origin.z;
        assert!(oldest + KEEP_TICKS as f64 + 1.0 >= 1000.0, "{oldest}");
        assert!(
            oldest + (MAX_ROUND_TRIP_TICKS + LAG_TICKS + WINDOW) <= 1000.0,
            "{oldest}"
        );
    }

    #[test]
    fn a_turn_interpolates_along_the_arc() {
        let turned = |deg: f64| {
            let h = deg.to_radians() / 2.0;
            Frame::from_quaternion(Vec3::ZERO, [0.0, h.sin(), 0.0, h.cos()])
        };
        let mid = turned(0.0).interpolate(&turned(10.0), 0.5);
        let want = turned(5.0);
        let p = Vec3::new(3.0, 1.0, 2.0);
        assert!(mid.to_world(p).distance(want.to_world(p)) < 1e-3);
    }

    #[test]
    fn footing_is_standing_on_a_top_above_it_and_nothing_inside_or_under() {
        let deck = deck();
        let f = |y: f64, x: f64| footing(&deck, Vec3::new(x, y, 0.0), BODY);
        assert_eq!(f(0.0, 0.0), Some(Footing::Standing));
        assert_eq!(f(0.04, 0.0), Some(Footing::Standing));
        assert_eq!(f(0.5, 0.0), Some(Footing::Above));
        assert_eq!(f(-0.2, 0.0), None, "inside the deck");
        assert_eq!(f(-3.0, 0.0), None, "under it");
        assert_eq!(f(0.0, 1.7), Some(Footing::Standing), "over its edge margin");
        assert_eq!(f(0.0, 2.0), None, "past the edge");
    }

    #[test]
    fn a_player_on_the_deck_fits_at_the_age_the_client_had() {
        let log = train(40, 2.0);
        // The client saw the deck 6 ticks old: it stands on it where it was then.
        let on_it = Vec3::new(0.4, 64.0, 2.0 * 34.0 + 3.0);
        let ages = Ages::new(100, None);
        assert_eq!(ages.anchor, 4.0);
        let fit = judge(&log, 40, &deck(), on_it, BODY, ages, true).unwrap();
        // Any age fits a long deck, so the one nearest the anchor wins: the anchor itself.
        assert_eq!(fit.age, 4.0);
        assert!((fit.local.z - (on_it.z - log.frame_at(40, 4.0).unwrap().origin.z)).abs() < 1e-9);
        // A hint moves where the search starts and is held to the window.
        assert_eq!(
            judge(
                &log,
                40,
                &deck(),
                on_it,
                BODY,
                Ages::new(100, Some(5)),
                true
            )
            .unwrap()
            .age,
            7.0
        );
        assert_eq!(Ages::new(100, Some(200)).anchor, Ages::new(100, None).hi);
    }

    #[test]
    fn a_position_beyond_the_window_does_not_fit() {
        let log = train(40, 2.0);
        // 30 blocks ahead of the deck at every age the window reaches (it is 20 long).
        let ahead = Vec3::new(0.0, 64.0, 2.0 * 40.0 + 30.0);
        assert!(judge(&log, 40, &deck(), ahead, BODY, Ages::new(100, None), false).is_none());
        // Beside it, or under it.
        let beside = Vec3::new(5.0, 64.0, 70.0);
        assert!(judge(&log, 40, &deck(), beside, BODY, Ages::new(100, None), false).is_none());
    }

    #[test]
    fn the_window_is_capped_at_a_second_of_round_trip() {
        let normal = Ages::new(1000, None);
        let far = Ages::new(60_000, None);
        assert_eq!(normal, far);
        assert_eq!(far.hi, MAX_ROUND_TRIP_TICKS + LAG_TICKS + WINDOW);
        assert_eq!(Ages::new(-5, None).lo, 0.0);
    }

    /// Arbitrary reports never fit unless they are on the deck at some age in the window: the
    /// accepted position, in the frame it fitted, is always over the deck and never inside it.
    #[test]
    fn no_report_fits_off_the_deck_or_through_it() {
        let log = train(60, 2.0);
        let deck = deck();
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut accepted = 0;
        for _ in 0..20_000 {
            let to = Vec3::new(
                next() * 12.0 - 6.0,
                63.0 + next() * 3.0,
                100.0 + next() * 40.0 - 20.0,
            );
            let rtt = (next() * 1200.0) as i32;
            let hint = (next() < 0.5).then(|| (next() * 40.0) as u8);
            let standing_only = next() < 0.5;
            let Some(fit) = judge(
                &log,
                60,
                &deck,
                to,
                BODY,
                Ages::new(rtt, hint),
                standing_only,
            ) else {
                continue;
            };
            accepted += 1;
            assert!(fit.local.x.abs() <= 1.5 + EDGE_MARGIN + 1e-9, "{fit:?}");
            assert!(fit.local.z.abs() <= 10.0 + EDGE_MARGIN + 1e-9, "{fit:?}");
            assert!(
                fit.local.y >= -TOP_TOLERANCE - 1e-9,
                "inside the deck: {fit:?}"
            );
            assert!((fit.age >= 0.0) && fit.age <= MAX_ROUND_TRIP_TICKS + LAG_TICKS + WINDOW);
            if standing_only {
                assert!(fit.local.y.abs() <= TOP_TOLERANCE + 1e-9, "{fit:?}");
            }
        }
        assert!(
            accepted > 1000,
            "the generator reaches the deck: {accepted}"
        );
    }
}
