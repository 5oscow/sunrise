//! Moving colliders on a real game: set, read, refused, saved and loaded back, and solid to a
//! falling item.
use super::{Dim, Game, native_colliders::SAVE_KEY};
use mod_api::{BlockStateId, Collider, ColliderBox, ColliderError, EntityRef};
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
            state: BlockStateId(1),
        }],
        rotation: [0.0, 0.0, 0.0, 1.0],
    }
}

/// A pig with no AI and no gravity at `at`: it stays where it is put.
fn stand(game: &mut Game<'_>, at: [f64; 3]) -> EntityRef {
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
        .expect("an armor stand");
    game.entity(id).expect("its handle")
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
