//! Moving colliders on a real game: set, read, refused, saved and loaded back, and solid to a
//! falling item.
use super::{Dim, Game, native_colliders::SAVE_KEY};
use mod_api::{Collider, ColliderBox, ColliderError, EntityRef};
use std::sync::Arc;
use sunrise_client::chunk::Chunk;
use sunrise_client::{entities::ItemEntity, items::ItemStack};
use sunrise_core::{
    geometry::Vec3,
    nbt::{Compound, Tag},
};

fn deck() -> Collider {
    Collider {
        boxes: vec![ColliderBox {
            min: [-1.5, -1.0, -1.5],
            max: [1.5, 0.0, 1.5],
        }],
        rotation: [0.0, 0.0, 0.0, 1.0],
    }
}

/// A pig with no AI and no gravity at `at`: it stays where it is put.
fn stand(game: &mut Game<'_>, at: [f64; 3]) -> EntityRef {
    stand_with_id(game, at).1
}

fn stand_with_id(game: &mut Game<'_>, at: [f64; 3]) -> (u32, EntityRef) {
    let nbt: Compound = [
        ("NoGravity".into(), Tag::Byte(1)),
        ("NoAI".into(), Tag::Byte(1)),
        ("Silent".into(), Tag::Byte(1)),
    ]
    .into_iter()
    .collect();
    let dimension = game.dim.id();
    let id = game
        .summon(dimension, "minecraft:pig", at, nbt)
        .expect("a pig");
    (id, game.entity(id).expect("its handle"))
}

#[test]
#[ignore = "daily"]
fn a_collider_is_set_read_refused_saved_and_loaded() {
    with_world(|game| {
        let entity = stand(game, [1.5, 70.0, 1.5]);
        assert_eq!(entity.collider(game), None);
        assert_eq!(entity.set_collider(game, Some(deck())), Ok(()));
        assert_eq!(entity.collider(game), Some(deck()));

        // Every refusal is by name, and the collider that was set stays.
        let mut bad = deck();
        bad.boxes[0].max[1] = f64::NAN;
        assert_eq!(
            entity.set_collider(game, Some(bad)),
            Err(ColliderError::NonFinite { index: Some(0) })
        );
        let mut far = deck();
        far.boxes[0].max[0] = Collider::MAX_REACH + 1.0;
        assert_eq!(
            entity.set_collider(game, Some(far)),
            Err(ColliderError::OutOfRange { index: 0 })
        );
        let mut many = deck();
        many.boxes = vec![many.boxes[0]; Collider::MAX_BOXES + 1];
        assert_eq!(
            entity.set_collider(game, Some(many)),
            Err(ColliderError::TooManyBoxes {
                count: Collider::MAX_BOXES as u64 + 1,
                limit: Collider::MAX_BOXES as u64
            })
        );
        let mut turned = deck();
        turned.rotation = [0.0, 0.0, 0.0, 3.0];
        assert_eq!(
            entity.set_collider(game, Some(turned)),
            Err(ColliderError::NonFinite { index: None })
        );
        assert_eq!(entity.collider(game), Some(deck()));
        let epoch = entity.epoch();
        assert_eq!(
            EntityRef::new(epoch, 999_999).set_collider(game, Some(deck())),
            Err(ColliderError::StaleEntity)
        );

        // The rotation alone.
        let half = std::f32::consts::FRAC_1_SQRT_2;
        assert_eq!(
            entity.set_collider_transform(game, [0.0, half, 0.0, half]),
            Ok(())
        );
        assert_eq!(
            entity.collider(game).unwrap().rotation,
            [0.0, half, 0.0, half]
        );
        assert_eq!(
            entity.set_collider_transform(game, [f32::NAN, 0.0, 0.0, 1.0]),
            Err(ColliderError::NonFinite { index: None })
        );
        let other = stand(game, [10.5, 70.0, 10.5]);
        assert_eq!(
            other.set_collider_transform(game, [0.0, 0.0, 0.0, 1.0]),
            Err(ColliderError::NoCollider)
        );

        // Saved with the entity, and read back by the entity that loads it.
        let id = entity.id();
        let mut tag = Tag::Compound(Compound::new());
        super::persist::write_attachments(&mut tag, id, &game.entity_attachments);
        assert!(tag.get(SAVE_KEY).is_some(), "the save holds the collider");
        let want = entity.collider(game);
        assert_eq!(entity.set_collider(game, None), Ok(()));
        assert_eq!(entity.collider(game), None);
        let mut cleared = Tag::Compound(Compound::new());
        super::persist::write_attachments(&mut cleared, id, &game.entity_attachments);
        assert!(cleared.get(SAVE_KEY).is_none(), "none is not saved");
        game.load_saved_collider(id, &tag);
        assert_eq!(entity.collider(game), want);
        // A malformed save is dropped, and leaves none.
        assert_eq!(entity.set_collider(game, None), Ok(()));
        let broken = Tag::compound([(SAVE_KEY, Tag::compound([("rotation", Tag::Int(1))]))]);
        game.load_saved_collider(id, &broken);
        assert_eq!(entity.collider(game), None);
    });
}

/// A stone dropped at `at`, two blocks above it, in the level in place: its id.
fn drop_item(game: &mut Game<'_>, at: [f64; 3]) -> u32 {
    game.next_entity += 1;
    let id = game.next_entity;
    game.dropped.push(ItemEntity::new(
        id,
        ItemStack::new("minecraft:stone", 1),
        Vec3::new(at[0], at[1] + 2.0, at[2]),
        Vec3::ZERO,
        0.0,
    ));
    id
}

fn item_y(game: &Game<'_>, id: u32) -> Option<f64> {
    game.dropped
        .iter()
        .find(|i| i.id == id)
        .map(|i| i.position.y)
}

/// An item dropped over a collider rests on its top: the one collision path every mover takes
/// finds the collider, placed at the start of the tick.
#[test]
#[ignore = "daily"]
fn a_dropped_item_rests_on_a_collider() {
    with_world(|game| {
        let entity = stand(game, [1.5, 70.0, 1.5]);
        entity.set_collider(game, Some(deck())).unwrap();
        let held = drop_item(game, [1.5, 70.0, 1.5]);
        for _ in 0..60 {
            tick_level(game);
        }
        let y = item_y(game, held).unwrap();
        assert!(
            (70.0..70.2).contains(&y),
            "the item rests on the top: y = {y}"
        );
        // Without the collider the same drop falls past that height.
        entity.set_collider(game, None).unwrap();
        let falling = drop_item(game, [1.5, 70.0, 1.5]);
        for _ in 0..60 {
            tick_level(game);
        }
        let y = item_y(game, falling).unwrap();
        assert!(y < 69.9, "nothing holds it up: y = {y}");
    });
}

/// Each level ticks with its own colliders: an item over the overworld's collider rests on it,
/// and an item at the same place in the nether, which has none there, falls past it.
#[test]
#[ignore = "daily"]
fn each_level_collides_with_its_own_colliders() {
    with_world(|game| {
        let spot = [1.5, 70.0, 1.5];
        let other = [10.5, 70.0, 10.5];
        let overworld = stand(game, spot);
        overworld.set_collider(game, Some(deck())).unwrap();
        game.enter_level(Dim::Nether).unwrap();
        game.native_level_opened(Dim::Nether);
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.streaming.forced.insert((0, 0));
        let nether = stand(game, other);
        nether.set_collider(game, Some(deck())).unwrap();
        // The nether's items: one over its own collider, one over the overworld's place.
        let (n_own, n_foreign) = (drop_item(game, other), drop_item(game, spot));
        for _ in 0..60 {
            tick_level(game);
        }
        game.enter_level(Dim::Overworld).unwrap();
        // The overworld's: one over its own collider, one over the nether's place.
        let (o_own, o_foreign) = (drop_item(game, spot), drop_item(game, other));
        for _ in 0..60 {
            tick_level(game);
        }
        let rests = |y: Option<f64>| y.is_some_and(|y| (70.0..70.2).contains(&y));
        assert!(rests(item_y(game, o_own)), "{:?}", item_y(game, o_own));
        assert!(
            item_y(game, o_foreign).is_none_or(|y| y < 69.9),
            "the nether's collider holds no overworld item: {:?}",
            item_y(game, o_foreign)
        );
        game.enter_level(Dim::Nether).unwrap();
        assert!(rests(item_y(game, n_own)), "{:?}", item_y(game, n_own));
        assert!(
            item_y(game, n_foreign).is_none_or(|y| y < 69.9),
            "the overworld's collider holds no nether item: {:?}",
            item_y(game, n_foreign)
        );
        game.enter_level(Dim::Overworld).unwrap();
    });
}

/// A game that simulates its overworld, with the chunk the tests build on loaded.
fn with_world(test: impl FnOnce(&mut Game<'_>)) {
    super::native_entity_refs::tests::with_game(|game| {
        let _world = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
        game.options.dedicated = Some(Vec::new());
        game.native_level_opened(Dim::Overworld);
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.streaming.forced.insert((0, 0));
        game.native_joins_skip();
        test(game);
    });
}

/// One tick of the level in place, as `tick_server` runs it: with the level's colliders placed.
fn tick_level(game: &mut Game<'_>) {
    let placed = game.placed_moving_colliders();
    sunrise_client_render::moving_colliders::in_level(placed, || game.tick_world(true).unwrap());
}

/// A collider goes with its entity through a dimension change: the same collider reads back
/// from the arrived entity, and it still holds an item up in the destination.
#[test]
#[ignore = "daily"]
fn a_collider_travels_with_its_entity() {
    with_world(|game| {
        let source = game.dim;
        let (id, entity) = stand_with_id(game, [1.5, 70.0, 1.5]);
        entity.set_collider(game, Some(deck())).unwrap();
        let want = entity.collider(game);
        game.enter_level(Dim::Nether).unwrap();
        game.native_level_opened(Dim::Nether);
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        game.streaming.forced.insert((0, 0));
        game.arrive_vehicle(source, id, Vec3::new(1.5, 70.0, 1.5), 0.0);
        for _ in 0..3 {
            tick_level(game);
        }
        let arrived = game.entity(id).expect("it arrived");
        assert_eq!(arrived.collider(game), want, "the collider came along");
        let held = drop_item(game, [1.5, 70.0, 1.5]);
        for _ in 0..60 {
            tick_level(game);
        }
        let y = item_y(game, held).unwrap();
        assert!(
            (70.0..70.2).contains(&y),
            "it still holds an item up: y = {y}"
        );
    });
}

/// The collider's own entity is never stopped by it, and neither is anything in its vehicle
/// stack, ancestors and siblings included; a stranger over the same boxes is.
#[test]
#[ignore = "daily"]
fn a_collider_is_not_solid_to_its_own_vehicle_stack() {
    with_world(|game| {
        let (a, a_ref) = stand_with_id(game, [1.5, 70.0, 1.5]);
        let (b, b_ref) = stand_with_id(game, [1.5, 70.0, 1.5]);
        let (c, c_ref) = stand_with_id(game, [1.5, 70.0, 1.5]);
        let (stranger, _) = stand_with_id(game, [10.5, 70.0, 10.5]);
        // a rides b rides c.
        game.entity_start_riding(a_ref, b_ref, true).unwrap();
        game.entity_start_riding(b_ref, c_ref, true).unwrap();
        // The collider is on the middle of the stack.
        b_ref.set_collider(game, Some(deck())).unwrap();
        for held in [a, b, c] {
            let mut stack = game.vehicle_stack(held);
            stack.sort_unstable();
            assert_eq!(stack, vec![a, b, c], "the stack seen from {held}");
        }
        assert_eq!(game.vehicle_stack(stranger), vec![stranger]);
        // What a mover of each kind meets where the collider stands.
        let placed = game.placed_moving_colliders();
        assert_eq!(placed.len(), 1);
        let mut exempt = placed.exempt(0).unwrap().to_vec();
        exempt.sort_unstable();
        assert_eq!(exempt, vec![a, b, c]);
        let mut stands_on = |id: u32| {
            use sunrise_client_render::moving_colliders::{in_level, near};
            let mut found = Vec::new();
            in_level(game.placed_moving_colliders(), || {
                near(
                    sunrise_world::entity_obstruction::EntityIdentity(u64::from(id)),
                    sunrise_core::geometry::Aabb::new(
                        Vec3::new(1.0, 69.5, 1.0),
                        Vec3::new(2.0, 71.0, 2.0),
                    ),
                    &mut found,
                );
            });
            !found.is_empty()
        };
        assert!(!stands_on(a) && !stands_on(b) && !stands_on(c));
        assert!(stands_on(stranger), "a stranger meets it");
    });
}

/// An item is a mover that carries its own collider: it moves through the gated collision as
/// a self move, so the boxes under its own feet are not solid to it and it falls through them,
/// where a stranger over the same boxes lands.
#[test]
#[ignore = "daily"]
fn an_item_falls_through_its_own_collider_and_a_stranger_lands_on_it() {
    let deck_under = || Collider {
        boxes: vec![ColliderBox {
            min: [-1.0, -2.5, -1.0],
            max: [1.0, -2.0, 1.0],
        }],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    with_world(|game| {
        // A holder that stays where it is, with the deck's top at y = 70, and a stranger over it.
        let fixed = drop_item(game, [1.5, 70.0, 1.5]);
        game.dropped
            .iter_mut()
            .find(|i| i.id == fixed)
            .unwrap()
            .no_gravity = true;
        game.entity(fixed)
            .unwrap()
            .set_collider(game, Some(deck_under()))
            .unwrap();
        let stranger = drop_item(game, [1.2, 71.0, 1.2]);
        // A holder that falls, with the same deck under its own feet.
        let own = drop_item(game, [10.5, 70.0, 10.5]);
        game.entity(own)
            .unwrap()
            .set_collider(game, Some(deck_under()))
            .unwrap();
        for _ in 0..60 {
            tick_level(game);
        }
        let other = item_y(game, stranger).unwrap();
        assert!(
            (70.0..70.3).contains(&other),
            "the stranger stands on the fixed holder's deck: y = {other}"
        );
        // The falling holder took its deck with it and was never stopped by it: with the deck
        // a block and a half under its feet, a stopped holder would rest at y = 72 - 2 + ... 70.
        let fell = item_y(game, own).unwrap_or(f64::NEG_INFINITY);
        assert!(
            fell < 60.0,
            "the holder fell through its own deck: y = {fell}"
        );
    });
}

/// Siblings in one vehicle stack are exempt from each other's collider, and a mover outside
/// the stack is not.
#[test]
#[ignore = "daily"]
fn siblings_riding_one_vehicle_share_a_stack() {
    with_world(|game| {
        let (x, x_ref) = stand_with_id(game, [1.5, 70.0, 1.5]);
        let (y, y_ref) = stand_with_id(game, [1.5, 70.0, 1.5]);
        let (z, z_ref) = stand_with_id(game, [1.5, 70.0, 1.5]);
        let (other, _) = stand_with_id(game, [10.5, 70.0, 10.5]);
        game.entity_start_riding(x_ref, z_ref, true).unwrap();
        game.entity_start_riding(y_ref, z_ref, true).unwrap();
        x_ref.set_collider(game, Some(deck())).unwrap();
        for member in [x, y, z] {
            let mut stack = game.vehicle_stack(member);
            stack.sort_unstable();
            let mut want = vec![x, y, z];
            want.sort_unstable();
            assert_eq!(stack, want, "seen from {member}");
        }
        assert_eq!(game.vehicle_stack(other), vec![other]);
        // What the exemption does: the collision query leaves the siblings and the vehicle
        // out and meets a stranger.
        let placed = game.placed_moving_colliders();
        let meets = |id: u32| {
            let mut found = Vec::new();
            sunrise_client_render::moving_colliders::in_level(placed.clone(), || {
                sunrise_client_render::moving_colliders::near(
                    sunrise_world::entity_obstruction::EntityIdentity(u64::from(id)),
                    sunrise_core::geometry::Aabb::new(
                        Vec3::new(1.0, 69.5, 1.0),
                        Vec3::new(2.0, 71.0, 2.0),
                    ),
                    &mut found,
                );
            });
            !found.is_empty()
        };
        assert!(!meets(x) && !meets(y) && !meets(z), "the stack is exempt");
        assert!(meets(other), "a stranger meets it");
    });
}

/// What a level's tick costs with 200 items over a collider at the box cap: with no collider,
/// with one that stays, and with one that moves every tick (Create's normal case), each timed
/// over the same ticks. The numbers go to stderr (`--nocapture`); the test holds the
/// outcome: the items rest on the capped collider, moving or not.
#[test]
#[ignore = "daily"]
fn two_hundred_items_over_a_capped_collider_rest_and_the_tick_is_timed() {
    use std::time::Instant;
    const SIDE: usize = 32;
    with_world(|game| {
        let (holder, holder_ref) = stand_with_id(game, [8.0, 70.0, 8.0]);
        let items = |game: &mut Game<'_>| -> Vec<u32> {
            (0..200)
                .map(|i| {
                    let at = [0.5 + (i % 15) as f64, 70.0, 0.5 + (i / 15) as f64];
                    drop_item(game, at)
                })
                .collect()
        };
        let time =
            |game: &mut Game<'_>, ticks: usize, mut each: Option<&mut dyn FnMut(&mut Game<'_>)>| {
                let start = Instant::now();
                for _ in 0..ticks {
                    if let Some(each) = each.as_mut() {
                        each(game);
                    }
                    tick_level(game);
                }
                start.elapsed().as_secs_f64() * 1000.0 / ticks as f64
            };
        // No collider: the items fall in the open.
        let before = items(game);
        let open = time(game, 40, None);
        for id in &before {
            game.dropped.retain(|i| i.id != *id);
        }
        // The cap: a 32 x 32 x 32 lump, its top at the holder's feet.
        let boxes = (0..SIDE * SIDE * SIDE)
            .map(|i| {
                let (x, y, z) = (i % SIDE, i / SIDE % SIDE, i / SIDE / SIDE);
                ColliderBox {
                    min: [x as f64 - 16.0, y as f64 - 32.0, z as f64 - 16.0],
                    max: [x as f64 - 15.0, y as f64 - 31.0, z as f64 - 15.0],
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(boxes.len(), Collider::MAX_BOXES);
        holder_ref
            .set_collider(
                game,
                Some(Collider {
                    boxes,
                    rotation: [0.0, 0.0, 0.0, 1.0],
                }),
            )
            .unwrap();
        let on = items(game);
        for _ in 0..20 {
            tick_level(game);
        }
        let rests = |game: &Game<'_>| {
            on.iter()
                .filter(|id| item_y(game, **id).is_some_and(|y| (70.0..70.3).contains(&y)))
                .count()
        };
        assert_eq!(rests(game), 200, "every item rests on the capped collider");
        let still = time(game, 40, None);
        assert_eq!(rests(game), 200);
        // The collider moves every tick: its placement is rebuilt each time.
        let index = game.mobs.iter().position(|m| m.id == holder).unwrap();
        let mut step = |game: &mut Game<'_>| {
            game.mobs.update(index, |m| m.position.x += 0.001);
        };
        let moving = time(game, 40, Some(&mut step));
        assert_eq!(rests(game), 200, "and they still rest on it while it moves");
        eprintln!(
            "tick of the level, 200 items, ms per tick: no collider {open:.3}, capped collider still {still:.3}, capped collider moving {moving:.3}"
        );
    });
}
