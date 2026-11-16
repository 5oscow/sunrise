use super::*;
use crate::mobs::world::with_built_test_world;
use std::sync::Arc;
use sunrise_world::registry::BlockRegistry;

/// A stone floor over the loaded 2x2 chunks (so a teleport finds ground),
/// `under` below the mob's spot and `at` in it.
fn build(
    under: &'static str,
    at: &'static str,
) -> impl FnOnce(&mut crate::world::World, &BlockRegistry) {
    move |world, registry| {
        let stone = registry.default_state("minecraft:stone").unwrap();
        for cx in -1..1 {
            for cz in -1..1 {
                world
                    .chunks
                    .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
            }
        }
        for x in -16..16 {
            for z in -16..16 {
                world.set(x, 63, z, stone).unwrap();
            }
        }
        world
            .set(0, 63, 0, registry.default_state(under).unwrap())
            .unwrap();
        world
            .set(0, 64, 0, registry.default_state(at).unwrap())
            .unwrap();
    }
}

fn enderman(types: &crate::mobs::MobTypes, seed: i64) -> Mob {
    let kind = Arc::new(types.types["minecraft:enderman"].clone());
    let mut m = Mob::new(1, kind, Vec3::new(0.5, 64.0, 0.5), 0.0);
    register(&mut m);
    m.random = sunrise_core::random::LegacyRandom::new(seed);
    m.on_ground = true;
    m.stuck_multiplier = Vec3::ZERO;
    m
}

/// `Entity.applyEffectsFromBlocks` fixes the tick's movements before
/// `stepOn`: when the magma's hurt teleports an enderman, the scan still
/// walks the path it had, and the cobweb it stood in still holds it.
#[test]
#[ignore = "daily"]
fn a_magma_teleport_leaves_the_block_scan_on_the_old_path() {
    assert!(
        with_built_test_world(
            &[],
            build("minecraft:magma_block", "minecraft:cobweb"),
            |w, types| {
                let mut teleported = 0;
                for seed in 0..40 {
                    let mut m = enderman(types, seed);
                    let mut test_level = crate::health::TestLevel::new(42);
                    let mut random = test_level.level();
                    let mut events = Vec::new();
                    effects_from_blocks(&mut m, w, &mut random, &mut events);
                    if m.position != Vec3::new(0.5, 64.0, 0.5) {
                        teleported += 1;
                        assert_eq!(
                            m.stuck_multiplier,
                            Vec3::new(0.25, f64::from(0.05_f32), 0.25),
                            "seed {seed}: the cobweb on the old path"
                        );
                    }
                }
                assert!(teleported > 0, "some seed teleports");
            },
        ),
        "vanilla data is missing"
    );
}

/// A hurt in `baseTick` that teleports an enderman (`snapTo` resets the
/// old position) leaves aiStep's fire contact at the landing spot: no
/// fire there, so the fire timer only counts down (no re-ignite to 160).
#[test]
#[ignore = "daily"]
fn a_burning_teleport_does_not_take_the_old_spots_fire_along() {
    assert!(
        with_built_test_world(
            &[],
            build("minecraft:stone", "minecraft:fire"),
            |w, types| {
                let mut teleported = 0;
                for seed in 0..40 {
                    let mut m = enderman(types, seed);
                    m.fire_ticks = 20;
                    // Whether the burning hurt's own teleport lands (one in ten
                    // draws none, and a try can find no ground: the old spot's
                    // fire then ignites it before the second hurt teleports it).
                    let mut first = m.clone();
                    let mut events = Vec::new();
                    hurt(
                        &mut first,
                        w,
                        &mut events,
                        1.0,
                        Damage::of(DamageKind::OnFire),
                    );
                    base_tick(
                        &mut m,
                        w,
                        &mut crate::health::TestLevel::new(0).level(),
                        &mut events,
                    );
                    if first.position != Vec3::new(0.5, 64.0, 0.5) {
                        assert_eq!(m.position, first.position, "seed {seed}: the same teleport");
                        teleported += 1;
                        assert_eq!(m.fire_ticks, 19, "seed {seed}: no fire where it landed");
                    }
                }
                assert!(teleported > 0, "some seed teleports");
            },
        ),
        "vanilla data is missing"
    );
}

/// How many generator steps (`next(bits)` calls: `next_double` is two)
/// `start` took to reach `end`.
fn steps(
    start: &sunrise_core::random::LegacyRandom,
    end: &sunrise_core::random::LegacyRandom,
) -> u32 {
    use sunrise_core::random::RandomSource as _;
    let mut r = start.clone();
    let mut n = 0;
    while r.state() != end.state() {
        r.next_int();
        n += 1;
        assert!(n < 1000, "the draws never meet");
    }
    n
}

fn hurt_sounds(events: &[MobEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, MobEvent::Sound("entity.enderman.hurt", ..)))
        .count()
}

/// #1608: `LivingEntity.tickEffects` hurts an enderman twice in one tick
/// (Wither, then Poison inside the hurt cooldown). `Enderman.hurtServer`
/// draws and teleports inside each hurt (the oracle's trace, `mob_ai`
/// `enderman_two_hurts_a_tick` tick 1): the hurt sound's pitch (2 draws),
/// `nextInt(10)` and, unless it is 0, `teleport()`'s x, y and z (5 draws);
/// then the refused Poison hurt: no sound, `nextInt(10)` and its teleport.
#[test]
#[ignore = "daily"]
fn two_effect_hurts_in_a_tick_each_teleport_inside_the_hurt() {
    use sunrise_core::random::RandomSource as _;
    assert!(
        with_built_test_world(
            &[],
            build("minecraft:stone", "minecraft:air"),
            |w, types| {
                let mut teleported = 0;
                for seed in 0..40 {
                    let mut m = enderman(types, seed);
                    m.add_effect("minecraft:wither", 40, 0);
                    m.add_effect("minecraft:poison", 50, 0);
                    let start = m.random.clone();
                    let mut events = Vec::new();
                    tick_effects_hurting(
                        &mut m,
                        w.loop_context,
                        &mut crate::health::TestLevel::new(0).level(),
                        w.mod_effects,
                        &mut |m| hurt_teleports(m, w, &mut events),
                    );
                    // A hurt with no teleport to settle leaves its sound in the outbox,
                    // which the tick hands on after the effects (`base_tick`).
                    events.append(&mut m.outbox);
                    // Vanilla's draws, counted from the same random.
                    let mut vanilla = start.clone();
                    vanilla.next_float();
                    vanilla.next_float();
                    for _ in 0..2 {
                        if vanilla.next_int_bound(10).unwrap() != 0 {
                            vanilla.next_double();
                            vanilla.next_int_bound(64).unwrap();
                            vanilla.next_double();
                        }
                    }
                    assert_eq!(
                        steps(&start, &m.random),
                        steps(&start, &vanilla),
                        "seed {seed}: vanilla's generator steps"
                    );
                    assert_eq!(
                        hurt_sounds(&events),
                        1,
                        "seed {seed}: the refused hurt is silent"
                    );
                    // The hurt sound leaves the outbox before the teleport's.
                    assert!(
                        matches!(
                            events.iter().find(|e| matches!(e, MobEvent::Sound(..))),
                            Some(MobEvent::Sound("entity.enderman.hurt", ..))
                        ),
                        "seed {seed}: the hurt sound comes first"
                    );
                    assert_eq!(
                        m.health,
                        m.attrs.max_health - 1.0,
                        "seed {seed}: one hurt lands"
                    );
                    if m.position != Vec3::new(0.5, 64.0, 0.5) {
                        teleported += 1;
                    }
                }
                assert!(teleported > 0, "some seed teleports");
            },
        ),
        "vanilla data is missing"
    );
}

/// #1608: `base_tick` settles each of its hurts inside the hurt. An enderman
/// hurt in a wall, below the world (`LivingEntity.onBelowWorld`) or by
/// drowning, then by Wither and Poison, ends where those three hurts, each
/// with its teleport, dealt one after the other leave it, with the same draws.
/// Below the world no teleport finds ground, so the draws alone tell.
#[test]
#[ignore = "daily"]
fn a_base_tick_hurt_and_two_effect_hurts_each_teleport_inside_the_hurt() {
    let world = |world: &mut crate::world::World, registry: &BlockRegistry| {
        build("minecraft:stone", "minecraft:air")(world, registry);
        // A stone at the eyes of an enderman standing at (0, 64, 0), and water
        // at the eyes of one standing at (8, 64, 8).
        let stone = registry.default_state("minecraft:stone").unwrap();
        world.set(0, 66, 0, stone).unwrap();
        let water = registry.default_state("minecraft:water").unwrap();
        for y in 64..68 {
            world.set(8, y, 8, water).unwrap();
        }
    };
    assert!(
        with_built_test_world(&[], world, |w, types| {
            let below = f64::from(w.world.bottom_y() - 65);
            let cases = [
                (
                    "in a wall",
                    Vec3::new(0.5, 64.0, 0.5),
                    DamageKind::InWall,
                    1.0,
                ),
                (
                    "below the world",
                    Vec3::new(0.5, below, 0.5),
                    DamageKind::OutOfWorld,
                    4.0,
                ),
                (
                    "drowning",
                    Vec3::new(8.5, 64.0, 8.5),
                    DamageKind::Drown,
                    2.0,
                ),
            ];
            for (name, at, kind, amount) in cases {
                let mut landed = 0;
                for seed in 0..40 {
                    let mut m = enderman(types, seed);
                    m.position = at;
                    if kind == DamageKind::Drown {
                        m.air = -19;
                    }
                    m.add_effect("minecraft:wither", 40, 0);
                    m.add_effect("minecraft:poison", 50, 0);
                    let start = m.random.clone();
                    let mut expected = m.clone();
                    let mut expected_events = Vec::new();
                    hurt(
                        &mut expected,
                        w,
                        &mut expected_events,
                        amount,
                        Damage::of(kind),
                    );
                    for kind in [DamageKind::Wither, DamageKind::Magic] {
                        hurt(
                            &mut expected,
                            w,
                            &mut expected_events,
                            1.0,
                            Damage::of(kind),
                        );
                    }
                    let mut events = Vec::new();
                    base_tick(
                        &mut m,
                        w,
                        &mut crate::health::TestLevel::new(0).level(),
                        &mut events,
                    );
                    assert_eq!(m.position, expected.position, "{name}, seed {seed}");
                    // Plus the ambient sound's roll at the end of `Mob.baseTick` (line
                    // 324 in the oracle's trace), after the effects.
                    assert_eq!(
                        steps(&start, &m.random),
                        steps(&start, &expected.random) + 1,
                        "{name}, seed {seed}: the same generator steps"
                    );
                    assert_eq!(
                        hurt_sounds(&events),
                        1,
                        "{name}, seed {seed}: one hurt sound"
                    );
                    if m.position != at {
                        landed += 1;
                    }
                }
                // Below the world no teleport finds ground; elsewhere some land.
                assert_eq!(
                    landed > 0,
                    kind != DamageKind::OutOfWorld,
                    "{name}: landings"
                );
            }
        }),
        "vanilla data is missing"
    );
}

/// #1608: an enderman Wither kills still draws `Enderman.hurtServer`'s
/// `nextInt(10)` in the refused hurt that follows (instant damage), and
/// `teleport()` draws nothing for a dead mob.
#[test]
#[ignore = "daily"]
fn a_dead_endermans_refused_hurt_still_draws_its_chance() {
    assert!(
        with_built_test_world(
            &[],
            build("minecraft:stone", "minecraft:air"),
            |w, types| {
                for seed in 0..10 {
                    let run = |second: bool| {
                        let mut m = enderman(types, seed);
                        m.health = 1.0;
                        m.add_effect("minecraft:wither", 40, 0);
                        if second {
                            m.add_effect("minecraft:instant_damage", 1, 0);
                        }
                        let start = m.random.clone();
                        let mut events = Vec::new();
                        tick_effects_hurting(
                            &mut m,
                            w.loop_context,
                            &mut crate::health::TestLevel::new(0).level(),
                            w.mod_effects,
                            &mut |m| hurt_teleports(m, w, &mut events),
                        );
                        assert!(!m.alive(), "seed {seed}: Wither kills it");
                        assert_eq!(
                            m.position,
                            Vec3::new(0.5, 64.0, 0.5),
                            "seed {seed}: no teleport"
                        );
                        steps(&start, &m.random)
                    };
                    assert_eq!(run(true), run(false) + 1, "seed {seed}: one more draw");
                }
            },
        ),
        "vanilla data is missing"
    );
}
