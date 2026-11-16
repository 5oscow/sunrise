//! The server's check of a player a moving collider carries (`lan/hub/carried_check.rs`): a
//! client moves its player with the collider as it last heard of it, some ticks old, and the
//! server judges each report in the collider's frames of the last ticks.
use super::*;
use std::{cell::RefCell, rc::Rc, sync::Arc, sync::atomic::Ordering};
use sunrise_client_render::moving_colliders::{History, HistoryScope, Placed, Prepared, in_level};
use sunrise_core::geometry::{
    Aabb, Vec3,
    oriented::{ColliderBoxes, Frame, OrientedCollider},
};

/// Where the deck's origin starts, above a floor whose top is at y 5.
const ORIGIN: [f64; 3] = [20.5, 9.0, 12.0];
/// The collider entities: not the guest's own entity id.
const DECK: u32 = 100;
const OTHER: u32 = 101;

/// A stone floor with its top at y 5, and `walls`: blocks (x, z) filled from the floor up.
fn scene(fixture: &crate::test_fixtures::Vanilla, walls: &[(i32, i32)]) -> World {
    let assets = &fixture.assets;
    let mut scene = World::test_scene(assets, &fixture.render, 10).unwrap();
    let stone = assets.registry.default_state("minecraft:stone").unwrap();
    let air = assets.registry.default_state("minecraft:air").unwrap();
    for x in 10..=30 {
        for z in 0..=170 {
            scene.set(x, 4, z, stone).unwrap();
            for y in 5..=14 {
                let wall = walls.contains(&(x, z)) && y < 14;
                scene.set(x, y, z, if wall { stone } else { air }).unwrap();
            }
        }
    }
    scene
}

fn deck(length: f64) -> Arc<ColliderBoxes> {
    Arc::new(ColliderBoxes::new(vec![Aabb::new(
        Vec3::new(-1.5, -1.0, -length / 2.0),
        Vec3::new(1.5, 0.0, length / 2.0),
    )]))
}

/// The server's side of one level: colliders that move, and the history of where they were.
struct Rig {
    history: Rc<RefCell<History>>,
    /// Each collider's entity, boxes and frame now.
    colliders: Vec<(u32, Arc<ColliderBoxes>, Frame)>,
}

impl Rig {
    fn new(colliders: Vec<(u32, Arc<ColliderBoxes>, Frame)>) -> Self {
        let mut rig = Self {
            history: Rc::default(),
            colliders,
        };
        rig.record();
        rig
    }

    fn prepared(&self) -> Prepared {
        Prepared::new(
            self.colliders
                .iter()
                .map(|(id, boxes, frame)| Placed {
                    id: *id,
                    collider: OrientedCollider::new(*frame, boxes.clone()),
                    exempt: vec![*id],
                })
                .collect(),
        )
    }

    fn record(&mut self) {
        self.history.borrow_mut().record(&self.prepared());
    }

    /// A server tick: every collider moves to `frame(entity)` and the level records them.
    fn tick(&mut self, frame: impl Fn(u32) -> Frame) {
        for (id, _, at) in &mut self.colliders {
            *at = frame(*id);
        }
        self.record();
    }

    /// Runs `f` as the level's tick holds its scopes.
    fn within<R>(&self, f: impl FnOnce() -> R) -> R {
        let _history = HistoryScope::enter(self.history.clone());
        in_level(self.prepared(), f)
    }

    /// The frame `age` ticks ago.
    fn frame(&self, id: u32, age: f64) -> Frame {
        let history = self.history.borrow();
        let (log, _) = history.get(id).unwrap();
        log.frame_at(history.now(), age).unwrap()
    }
}

fn along(z: f64) -> Frame {
    Frame::translation(Vec3::new(ORIGIN[0], ORIGIN[1], z))
}

/// A tiny deterministic generator.
struct Lcg(u64);

impl Lcg {
    fn unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

struct Guestside<'a> {
    hub: Hub,
    fixture: &'a crate::test_fixtures::Vanilla,
    world: World,
    border: crate::worldborder::WorldBorder,
}

impl<'a> Guestside<'a> {
    fn new(fixture: &'a crate::test_fixtures::Vanilla, walls: &[(i32, i32)], rtt_ms: i32) -> Self {
        let (mut hub, _rx) = hub_with_a_guest();
        hub.guests.update(0, |g| {
            g.latency.store(rtt_ms, Ordering::Relaxed);
            g.on_ground = true;
        });
        Self {
            hub,
            fixture,
            world: scene(fixture, walls),
            border: Default::default(),
        }
    }

    /// The server's answer to a report at `to`, and the guest takes it where the answer is yes
    /// (`accept_move`).
    fn report(&mut self, rig: &Rig, to: [f64; 3], hint: Option<u8>) -> (bool, bool, bool) {
        if let Some(ticks) = hint {
            self.hub
                .guests
                .update(0, |g| g.carry_hint = Some((DECK, ticks)));
        }
        let mut next = 2;
        let ctx = TickCtx {
            sea_level: 63,
            world: &mut self.world,
            entities: &[],
            next_entity: &mut next,
            spawn: [20.5, 9.0, 12.0],
            spawn_angle: [0.0; 2],
            difficulty: 2,
            difficulty_locked: false,
            clock_rate: 1.0,
            tick_rate: 20.0,
            tick_frozen: false,
            tick_steps: Default::default(),
            collision: Some(&self.fixture.collision),
            border: &self.border,
            streaming: None,
        };
        let hub = &mut self.hub;
        let answer = rig.within(|| hub.collide_carried(0, to, &ctx));
        if !answer.1 {
            self.hub.guests.update(0, |g| g.position = to);
        }
        answer
    }

    fn guest(&self) -> &Guest {
        &self.hub.guests[0]
    }
}

/// Where an honest client puts its player: the deck as it saw it `age` ticks ago.
fn honest(rig: &Rig, age: f64, local: Vec3) -> [f64; 3] {
    let at = rig.frame(DECK, age).to_world(local);
    [at.x, at.y, at.z]
}

fn first(fixture: &crate::test_fixtures::Vanilla) -> (Rig, Guestside<'_>) {
    let mut rig = Rig::new(vec![(DECK, deck(20.0), along(ORIGIN[2]))]);
    let mut side = Guestside::new(fixture, &[], 100);
    // Ten ticks standing still, the player on the deck.
    for _ in 0..10 {
        rig.tick(|_| along(ORIGIN[2]));
    }
    let to = honest(&rig, 4.0, Vec3::new(0.0, 0.0, 0.0));
    side.hub.guests.update(0, |g| g.position = to);
    (rig, side)
}

/// An honest rider is never refused, at any speed Create's trains run at, any round trip the
/// check allows for, jitter of a tick either way and several seeds: it attaches, stays
/// attached, is never counted as floating and no report is rejected.
#[test]
#[ignore = "daily"]
fn an_honest_rider_at_train_speed_is_never_rejected() {
    let Some(fixture) = crate::test_fixtures::vanilla() else {
        return;
    };
    let (mut spent, mut reports) = (std::time::Duration::ZERO, 0u32);
    for seed in [1, 2, 3] {
        for speed in [0.5, 1.4, 2.0] {
            for rtt in [50, 100, 200] {
                let mut rng = Lcg(seed * 7919 + (speed * 10.0) as u64 + rtt as u64);
                let mut rig = Rig::new(vec![(DECK, deck(20.0), along(ORIGIN[2]))]);
                let mut side = Guestside::new(fixture, &[], rtt);
                let trip = f64::from(rtt) / 50.0;
                let age = trip + 2.0;
                let mut local = Vec3::new(0.0, 0.0, 0.0);
                for t in 1..=60 {
                    rig.tick(|_| along(ORIGIN[2] + speed * f64::from(t)));
                    if t < 25 {
                        continue;
                    }
                    local = Vec3::new(
                        (local.x + (rng.unit() - 0.5) * 0.4).clamp(-1.2, 1.2),
                        0.0,
                        (local.z + (rng.unit() - 0.5) * 0.4).clamp(-8.0, 8.0),
                    );
                    // The jitter moves the copy the client held by a tick either way.
                    let jitter = (rng.unit() * 3.0).floor() - 1.0;
                    let to = honest(&rig, age + jitter, local);
                    if t == 25 {
                        side.hub.guests.update(0, |g| g.position = to);
                    }
                    let began = std::time::Instant::now();
                    let (_, refused, stands) = side.report(&rig, to, None);
                    spent += began.elapsed();
                    reports += 1;
                    assert!(
                        !refused,
                        "seed {seed} speed {speed} rtt {rtt} tick {t}: refused at {to:?}"
                    );
                    assert!(stands, "supported");
                    assert!(side.guest().on_collider.is_some());
                    assert!(side.guest().carried_standing);
                }
                assert_eq!(
                    side.guest().carry_rejects,
                    0,
                    "seed {seed} speed {speed} rtt {rtt}"
                );
            }
        }
    }
    eprintln!(
        "carried check: {reports} honest reports, {:.1} us each (the whole move check, a deck of one box)",
        spent.as_secs_f64() * 1.0e6 / f64::from(reports)
    );
}

/// The same on a curve: the deck turns 4 degrees a tick and its origin follows its heading,
/// at the speeds Create's trains turn at and beyond them.
#[test]
#[ignore = "daily"]
fn an_honest_rider_on_a_curve_is_never_rejected() {
    let Some(fixture) = crate::test_fixtures::vanilla() else {
        return;
    };
    for seed in [1, 2] {
        for speed in [0.7, 1.0, 2.0] {
            for rtt in [50, 100, 200] {
                let mut rng = Lcg(seed * 104_729 + (speed * 10.0) as u64 + rtt as u64);
                let mut rig = Rig::new(vec![(DECK, deck(20.0), along(ORIGIN[2]))]);
                let mut side = Guestside::new(fixture, &[], rtt);
                let age = f64::from(rtt) / 50.0 + 2.0;
                let (mut x, mut z) = (ORIGIN[0], ORIGIN[2]);
                let mut local = Vec3::new(0.0, 0.0, 0.0);
                for t in 1..=60 {
                    let heading = (4.0 * f64::from(t)).to_radians();
                    x += speed * heading.sin();
                    z += speed * heading.cos();
                    let half = heading / 2.0;
                    rig.tick(|_| {
                        Frame::from_quaternion(
                            Vec3::new(x, ORIGIN[1], z),
                            [0.0, half.sin(), 0.0, half.cos()],
                        )
                    });
                    if t < 25 {
                        continue;
                    }
                    local = Vec3::new(
                        (local.x + (rng.unit() - 0.5) * 0.4).clamp(-1.2, 1.2),
                        0.0,
                        (local.z + (rng.unit() - 0.5) * 0.4).clamp(-8.0, 8.0),
                    );
                    let jitter = (rng.unit() * 3.0).floor() - 1.0;
                    let to = honest(&rig, age + jitter, local);
                    if t == 25 {
                        side.hub.guests.update(0, |g| g.position = to);
                    }
                    let (_, refused, _) = side.report(&rig, to, None);
                    assert!(
                        !refused,
                        "seed {seed} speed {speed} rtt {rtt} tick {t}: refused at {to:?}"
                    );
                }
                assert_eq!(side.guest().carry_rejects, 0);
            }
        }
    }
}

/// A player who jumps onto a deck going 2.0 blocks a tick, two reports in a tick at times: the
/// fall is judged in the world the client saw, and it attaches the tick it lands.
#[test]
#[ignore = "daily"]
fn jumping_onto_a_deck_at_two_blocks_a_tick_attaches_on_landing() {
    let Some(fixture) = crate::test_fixtures::vanilla() else {
        return;
    };
    let mut rig = Rig::new(vec![(DECK, deck(20.0), along(ORIGIN[2]))]);
    let mut side = Guestside::new(fixture, &[], 100);
    for t in 1..=30 {
        rig.tick(|_| along(ORIGIN[2] + 2.0 * f64::from(t)));
    }
    // 1.2 blocks over the deck and 5 blocks to its side, falling and drifting onto it.
    let steps = [
        (4.0, 1.2, 1.9),
        (4.0, 0.9, 1.6),
        (4.0, 0.5, 1.2),
        (4.0, 0.2, 0.8),
        (4.0, 0.0, 0.5),
    ];
    let mut t = 30;
    for (i, (age, y, x)) in steps.into_iter().enumerate() {
        t += 1;
        rig.tick(|_| along(ORIGIN[2] + 2.0 * f64::from(t)));
        let to = honest(&rig, age, Vec3::new(x, y, 0.0));
        if i == 0 {
            side.hub.guests.update(0, |g| g.position = to);
        }
        let (_, refused, _) = side.report(&rig, to, None);
        assert!(!refused, "step {i} at {to:?}");
        // A second report in the same tick, a little further on.
        let again = honest(&rig, age, Vec3::new(x - 0.02, y, 0.0));
        assert!(!side.report(&rig, again, None).1, "step {i}, again");
        assert_eq!(
            side.guest().on_collider.is_some(),
            y.abs() < 0.05,
            "step {i}"
        );
    }
    assert!(side.guest().carried_standing);
    assert_eq!(side.guest().carry_rejects, 0);
}

/// A report that fits the deck at no age is corrected to where the server last had the player
/// on it, placed under the deck as it is now, and counted; arbitrary reports never leave the
/// player off the deck or inside it while it is carried.
#[test]
#[ignore = "daily"]
fn a_report_off_the_train_is_corrected_to_the_server_local_position_and_counted() {
    let Some(fixture) = crate::test_fixtures::vanilla() else {
        return;
    };
    let mut rig = Rig::new(vec![(DECK, deck(20.0), along(ORIGIN[2]))]);
    let mut side = Guestside::new(fixture, &[], 100);
    for t in 1..=40 {
        rig.tick(|_| along(ORIGIN[2] + 2.0 * f64::from(t)));
    }
    let local = Vec3::new(0.5, 0.0, 1.0);
    let to = honest(&rig, 4.0, local);
    side.hub.guests.update(0, |g| g.position = to);
    assert!(!side.report(&rig, to, None).1);
    let held = side.guest().on_collider.expect("attached");
    // 30 blocks ahead of the deck at every age the window reaches.
    let far = [to[0], to[1], to[2] + 60.0];
    let refused = side.report(&rig, far, None).1;
    // A free move of 60 blocks is not a carried move: the ordinary rules decide it. What the
    // carried check promises is the correction of one it refuses.
    if refused {
        let corrected = side.guest().carry_correction.expect("a correction");
        let now = rig.frame(DECK, 0.0).to_world(held.local);
        assert!(
            (corrected[0] - now.x).abs() < 1e-9 && (corrected[2] - now.z).abs() < 1e-9,
            "{corrected:?} vs {now:?}"
        );
        assert_eq!(side.guest().carry_rejects, 1);
    }
    // Arbitrary reports: whenever one is accepted while attached, the player is over the deck
    // in the frame it matched, never inside it.
    let mut rng = Lcg(99);
    let mut fits = 0;
    for _ in 0..2_000 {
        side.hub.guests.update(0, |g| {
            g.position = to;
            g.on_collider = Some(held);
        });
        let at = [
            to[0] + (rng.unit() - 0.5) * 8.0,
            to[1] + (rng.unit() - 0.5) * 3.0,
            to[2] + (rng.unit() - 0.5) * 40.0,
        ];
        let (_, refused, _) = side.report(&rig, at, None);
        let Some(now) = side.guest().on_collider else {
            continue;
        };
        if refused {
            continue;
        }
        fits += 1;
        assert!(now.local.x.abs() <= 1.8 + 1e-9, "{now:?}");
        assert!(now.local.z.abs() <= 10.3 + 1e-9, "{now:?}");
        assert!(now.local.y >= -0.05 - 1e-9, "inside the deck: {now:?}");
    }
    assert!(fits > 20, "the reports reach the deck: {fits}");
}

/// Stepping from one collider to another detaches from the first and attaches to the second,
/// each way, with no report refused; stepping onto the ground detaches too.
#[test]
#[ignore = "daily"]
fn stepping_between_colliders_hands_the_player_over_each_way() {
    let Some(fixture) = crate::test_fixtures::vanilla() else {
        return;
    };
    // Two decks side by side, 4 apart in x, moving at different speeds.
    let frames = |t: f64| {
        move |id: u32| match id {
            DECK => along(ORIGIN[2] + 1.0 * t),
            _ => Frame::translation(Vec3::new(ORIGIN[0] + 4.0, ORIGIN[1], ORIGIN[2] + 1.6 * t)),
        }
    };
    let mut rig = Rig::new(vec![
        (DECK, deck(20.0), along(ORIGIN[2])),
        (
            OTHER,
            deck(20.0),
            Frame::translation(Vec3::new(ORIGIN[0] + 4.0, ORIGIN[1], ORIGIN[2])),
        ),
    ]);
    let mut side = Guestside::new(fixture, &[], 100);
    for t in 1..=30 {
        rig.tick(frames(f64::from(t)));
    }
    let on = |rig: &Rig, id: u32, age: f64, local: Vec3| {
        let at = rig.frame(id, age).to_world(local);
        [at.x, at.y, at.z]
    };
    // On the first deck.
    let to = on(&rig, DECK, 4.0, Vec3::new(1.0, 0.0, 0.0));
    side.hub.guests.update(0, |g| g.position = to);
    assert!(!side.report(&rig, to, None).1);
    assert_eq!(side.guest().on_collider.map(|c| c.collider), Some(DECK));
    // Walks east off its edge (the margin ends at local x 1.8): airborne between the decks,
    // then on the second.
    let mut t = 30;
    for (id, local) in [
        (DECK, Vec3::new(1.6, 0.0, 0.0)),
        (DECK, Vec3::new(1.9, 0.4, 0.0)),
        (OTHER, Vec3::new(-1.4, 0.4, 0.0)),
        (OTHER, Vec3::new(-1.2, 0.0, 0.0)),
        (OTHER, Vec3::new(-1.0, 0.0, 0.0)),
    ] {
        t += 1;
        rig.tick(frames(f64::from(t)));
        let to = on(&rig, id, 4.0, local);
        let (_, refused, _) = side.report(&rig, to, None);
        assert!(!refused, "{id} {local:?}");
    }
    assert_eq!(side.guest().on_collider.map(|c| c.collider), Some(OTHER));
    // And back west onto the first.
    for (id, local) in [
        (OTHER, Vec3::new(-1.6, 0.0, 0.0)),
        (OTHER, Vec3::new(-1.9, 0.4, 0.0)),
        (DECK, Vec3::new(1.4, 0.4, 0.0)),
        (DECK, Vec3::new(1.2, 0.0, 0.0)),
        (DECK, Vec3::new(1.0, 0.0, 0.0)),
    ] {
        t += 1;
        rig.tick(frames(f64::from(t)));
        let to = on(&rig, id, 4.0, local);
        let (_, refused, _) = side.report(&rig, to, None);
        assert!(!refused, "{id} {local:?}");
    }
    assert_eq!(side.guest().on_collider.map(|c| c.collider), Some(DECK));
    assert_eq!(side.guest().carry_rejects, 0);
    // Off the deck's end onto the ground: it lets go.
    t += 1;
    rig.tick(frames(f64::from(t)));
    let ground = [to[0] + 8.0, 5.0, to[2] + 6.0];
    assert!(!side.report(&rig, ground, None).1);
    assert!(side.guest().on_collider.is_none());
}

/// A collider that brakes, one whose stream stalls and then bursts, and a round trip the
/// server mis-estimates by two ticks either way: the honest rider is not refused.
#[test]
#[ignore = "daily"]
fn braking_a_stall_with_a_burst_and_a_wrong_estimate_still_pass() {
    let Some(fixture) = crate::test_fixtures::vanilla() else {
        return;
    };
    // Braking from 2.0 blocks a tick to a stop over 10 ticks.
    let mut z = ORIGIN[2];
    let mut speed: f64 = 2.0;
    let mut rig = Rig::new(vec![(DECK, deck(20.0), along(z))]);
    let mut side = Guestside::new(fixture, &[], 100);
    let local = Vec3::new(0.0, 0.0, 0.0);
    for t in 1..=60 {
        if t > 30 {
            speed = (speed - 0.2).max(0.0);
        }
        z += speed;
        rig.tick(|_| along(z));
        if t >= 20 {
            let to = honest(&rig, 4.0, local);
            if t == 20 {
                side.hub.guests.update(0, |g| g.position = to);
            }
            assert!(!side.report(&rig, to, None).1, "braking tick {t}");
        }
    }
    // A stall: the client's copy stays where it was for 6 ticks while the server's deck goes
    // on at 2.0, then catches up. The client says its copy is that many ticks older.
    let mut rig = Rig::new(vec![(DECK, deck(20.0), along(ORIGIN[2]))]);
    let mut side = Guestside::new(fixture, &[], 100);
    for t in 1..=40 {
        rig.tick(|_| along(ORIGIN[2] + 2.0 * f64::from(t)));
    }
    let to = honest(&rig, 4.0, local);
    side.hub.guests.update(0, |g| g.position = to);
    assert!(!side.report(&rig, to, None).1);
    for stalled in 1..=3u8 {
        let t = 40 + u32::from(stalled);
        rig.tick(|_| along(ORIGIN[2] + 2.0 * f64::from(t)));
        // Its copy is still the one it had: another tick older each time.
        let to = honest(&rig, 4.0 + f64::from(stalled), local);
        assert!(
            !side.report(&rig, to, Some(2 + stalled)).1,
            "stalled {stalled}"
        );
    }
    // Every wrong estimate within two ticks passes: the server believes 100 ms, the truth is
    // 0 ms to 200 ms.
    for off in -2i32..=2 {
        let mut rig = Rig::new(vec![(DECK, deck(20.0), along(ORIGIN[2]))]);
        let mut side = Guestside::new(fixture, &[], 100);
        for t in 1..=40 {
            rig.tick(|_| along(ORIGIN[2] + 2.0 * f64::from(t)));
            if t >= 20 {
                let to = honest(&rig, 4.0 + f64::from(off), local);
                if t == 20 {
                    side.hub.guests.update(0, |g| g.position = to);
                }
                assert!(!side.report(&rig, to, None).1, "off {off} tick {t}");
            }
        }
    }
}

/// A collider that is gone lets its rider fall, and a teleport (a portal, a command) lets go.
#[test]
#[ignore = "daily"]
fn a_disassembled_collider_and_a_teleport_let_the_rider_go() {
    let Some(fixture) = crate::test_fixtures::vanilla() else {
        return;
    };
    let (mut rig, mut side) = first(fixture);
    let to = honest(&rig, 4.0, Vec3::new(0.0, 0.0, 0.0));
    assert!(!side.report(&rig, to, None).1);
    assert!(side.guest().on_collider.is_some());
    // The collider is removed: the history forgets it and the rider falls like anyone.
    rig.colliders.clear();
    rig.record();
    let fall = [to[0], to[1] - 0.08, to[2]];
    let (wrongly, refused, _) = side.report(&rig, fall, None);
    assert_eq!((wrongly, refused), (false, false));
    assert!(side.guest().on_collider.is_none());
    assert!(!side.guest().carried_standing);
    // A teleport lets go of a rider.
    let (rig, mut side) = first(fixture);
    let to = honest(&rig, 4.0, Vec3::new(0.0, 0.0, 0.0));
    assert!(!side.report(&rig, to, None).1);
    assert!(side.guest().on_collider.is_some());
    side.hub.resend_teleport(0, Some([50.5, 5.0, 50.5]));
    assert!(side.guest().on_collider.is_none());
}

/// Blocks beside the track still stop an honest rider, and a train through a tunnel is no
/// reason to refuse one; a report inside the deck is a collision with something new.
#[test]
#[ignore = "daily"]
fn walls_beside_the_track_and_the_deck_itself_still_stop_an_honest_rider() {
    let Some(fixture) = crate::test_fixtures::vanilla() else {
        return;
    };
    // A tunnel: walls at x 18 and x 22 (the deck spans x 19 to 22, so they touch it) along the whole track.
    let walls: Vec<(i32, i32)> = (0..=170).flat_map(|z| [(18, z), (22, z)]).collect();
    let mut rig = Rig::new(vec![(DECK, deck(20.0), along(ORIGIN[2]))]);
    let mut side = Guestside::new(fixture, &walls, 100);
    for t in 1..=60 {
        rig.tick(|_| along(ORIGIN[2] + 2.0 * f64::from(t)));
        if t >= 20 {
            let to = honest(
                &rig,
                4.0,
                Vec3::new(1.0 - 0.02 * f64::from(t % 10), 0.0, 0.0),
            );
            if t == 20 {
                side.hub.guests.update(0, |g| g.position = to);
            }
            assert!(!side.report(&rig, to, None).1, "the tunnel, tick {t}");
        }
    }
    // The player walks east into the wall at x 22: its client stops it there, so a report
    // beyond that is a wall the server refuses.
    let into = honest(&rig, 4.0, Vec3::new(1.45, 0.0, 0.0));
    let (_, refused, _) = side.report(&rig, into, None);
    assert!(refused, "through the wall at {into:?}");
    // Inside the deck: a collision the body was not in a moment ago.
    let (mut rig, mut side) = (
        Rig::new(vec![(DECK, deck(20.0), along(ORIGIN[2]))]),
        Guestside::new(fixture, &[], 100),
    );
    for _ in 0..10 {
        rig.tick(|_| along(ORIGIN[2]));
    }
    let on = honest(&rig, 4.0, Vec3::new(0.0, 0.0, 0.0));
    side.hub.guests.update(0, |g| g.position = on);
    assert!(!side.report(&rig, on, None).1);
    let inside = honest(&rig, 4.0, Vec3::new(0.0, -0.6, 0.0));
    assert!(side.report(&rig, inside, None).1, "inside the deck");
}

/// Past a second of round trip the window stops growing: a rider whose copy is older than the
/// window reaches is not carried, and is judged as any walker in the frame the cap allows.
#[test]
#[ignore = "daily"]
fn past_a_second_of_round_trip_the_rider_is_no_longer_carried() {
    let Some(fixture) = crate::test_fixtures::vanilla() else {
        return;
    };
    let mut rig = Rig::new(vec![(DECK, deck(20.0), along(ORIGIN[2]))]);
    let mut side = Guestside::new(fixture, &[], 3_000);
    for t in 1..=70 {
        rig.tick(|_| along(ORIGIN[2] + f64::from(t)));
    }
    // The truth is 3 s: the client's copy is 62 ticks old, and the server holds 26.
    let at = along(ORIGIN[2] + 70.0 - 62.0).to_world(Vec3::new(0.0, 0.0, 0.0));
    let to = [at.x, at.y, at.z];
    side.hub.guests.update(0, |g| g.position = to);
    side.report(&rig, to, None);
    assert!(side.guest().on_collider.is_none());
    assert!(!side.guest().carried_standing);
}
