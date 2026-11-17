//! #2223: the server's moved-wrongly check resolves a reported move against the level's solid
//! entities (`Entity.collide`'s entity collisions), so a client that walks through a shulker is
//! told it moved wrongly, and one that stops at its side is not.
use super::*;
use crate::entity_solids::{Candidate, SolidKind};
use sunrise_core::geometry::{Aabb, Vec3};

/// A stone floor with its top at y 5 and air above it.
fn floor(fixture: &crate::test_fixtures::Vanilla) -> World {
    let assets = &fixture.assets;
    let mut scene = World::test_scene(assets, &fixture.render, 2).unwrap();
    let stone = assets.registry.default_state("minecraft:stone").unwrap();
    let air = assets.registry.default_state("minecraft:air").unwrap();
    for x in 10..=30 {
        for z in 10..=30 {
            scene.set(x, 4, z, stone).unwrap();
            for y in 5..=12 {
                scene.set(x, y, z, air).unwrap();
            }
        }
    }
    scene
}

/// A shulker with its west face at x 22, standing on the floor: a full block high, so the
/// player's 0.6 step does not carry it over.
fn shulker() -> Candidate {
    Candidate::fixed(
        SolidKind::Shulker,
        500,
        Aabb::new(Vec3::new(22.0, 5.0, 20.0), Vec3::new(23.0, 6.0, 21.0)),
    )
}

#[test]
#[ignore = "daily"]
fn a_reported_move_through_a_shulker_is_moved_wrongly_and_one_that_stops_at_its_side_is_not() {
    let Some(fixture) = crate::test_fixtures::vanilla() else {
        return;
    };
    let mut world = floor(fixture);
    let (mut hub, _rx) = hub_with_a_guest();
    let mut next_entity = 2;
    let border = crate::worldborder::WorldBorder::default();
    let ctx = TickCtx {
        sea_level: 63,
        world: &mut world,
        entities: &[],
        next_entity: &mut next_entity,
        spawn: [20.5, 5.0, 20.5],
        spawn_angle: [0.0; 2],
        difficulty: 2,
        difficulty_locked: false,
        clock_rate: 1.0,
        tick_rate: 20.0,
        tick_frozen: false,
        tick_steps: Default::default(),
        collision: Some(&fixture.collision),
        border: &border,
        streaming: None,
    };
    hub.guests.update(0, |g| {
        g.position = [20.5, 5.0, 20.5];
        g.on_ground = true;
    });
    // The player's half width is 0.3: stopped by the boat, it stands at x 21.7.
    let through = [23.0, 5.0, 20.5];
    let beside = [21.7, 5.0, 20.5];

    // No entity: the move east is free, whatever the report.
    let (wrongly, refused, _) = hub.collide(0, through, &ctx);
    assert!(!wrongly && !refused, "no boat: {wrongly} {refused}");

    hub.move_solids = vec![shulker()];
    let (wrongly, refused, _) = hub.collide(0, through, &ctx);
    assert!(
        wrongly && refused,
        "through the entity: {wrongly} {refused}"
    );
    let (wrongly, refused, _) = hub.collide(0, beside, &ctx);
    assert!(
        !wrongly && !refused,
        "beside the entity: {wrongly} {refused}"
    );

    // An entity the move does not reach changes nothing.
    hub.move_solids = vec![Candidate::fixed(
        SolidKind::Shulker,
        501,
        Aabb::new(Vec3::new(26.0, 5.0, 20.0), Vec3::new(27.0, 6.0, 21.0)),
    )];
    let (wrongly, refused, _) = hub.collide(0, [22.5, 5.0, 20.5], &ctx);
    assert!(!wrongly && !refused, "a far entity: {wrongly} {refused}");
}
