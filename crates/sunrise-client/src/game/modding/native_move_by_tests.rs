//! `EntityRef::move_by` and the hanging entities `LevelRef::entities_in` lists.

use super::native_entity_refs::tests::with_game;
use mod_api::MoveCause;
use sunrise_client::entities::{ArmorStand, ItemFrame, Painting};
use sunrise_core::geometry::Vec3;

const STAND: u32 = 910;
const FRAME: u32 = 911;
const PAINTING: u32 = 912;

/// A server's game with an armor stand at 0.5, 64, 0.5, an item frame at 4, 70, 4 and a painting
/// at 4, 70, 6 (the frame's and painting's boxes are a block wide, thin in depth).
fn with_scene(test: impl FnOnce(&mut super::Game<'_>)) {
    with_game(|game| {
        game.options.dedicated = Some(Vec::new());
        game.armor_stands
            .push(ArmorStand::new(STAND, Vec3::new(0.5, 64.0, 0.5), 0.0));
        game.item_frames.push(ItemFrame {
            attachment_lifetime: Default::default(),
            join: Default::default(),
            id: FRAME,
            pos: [4, 70, 4],
            facing: 3,
            glow: false,
            item: None,
            rotation: 0,
            drop_chance: 1.0,
            fixed: false,
            invisible: false,
            custom_name: None,
            random: sunrise_core::random::LegacyRandom::new(0),
            check_interval: 0,
        });
        game.paintings.push(Painting {
            attachment_lifetime: Default::default(),
            join: Default::default(),
            id: PAINTING,
            pos: [4, 70, 6],
            facing: 0,
            variant: "minecraft:kebab".into(),
            width: 1,
            height: 1,
            check_interval: 0,
        });
        game.next_entity = game.next_entity.max(PAINTING);
        test(game);
    });
}

#[test]
#[ignore = "daily"]
fn an_area_query_lists_the_frames_and_paintings_in_the_box() {
    with_scene(|game| {
        let level = game.native_list_levels()[0];
        let frame = game.entity(FRAME).unwrap();
        let painting = game.entity(PAINTING).unwrap();
        let around = |game: &super::Game<'_>, min, max| game.level_entities_in(level, min, max);
        let both = around(game, [3.0, 69.0, 3.0], [5.0, 71.0, 7.0]).unwrap();
        assert!(both.contains(&frame) && both.contains(&painting));
        let one = around(game, [3.0, 69.0, 3.0], [5.0, 71.0, 5.0]).unwrap();
        assert!(one.contains(&frame) && !one.contains(&painting));
        let none = around(game, [10.0, 69.0, 3.0], [12.0, 71.0, 7.0]).unwrap();
        assert!(!none.contains(&frame) && !none.contains(&painting));
    });
}

#[test]
#[ignore = "daily"]
fn move_by_stops_at_a_wall_and_refuses_what_has_no_physics() {
    with_scene(|game| {
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        for y in 63..67 {
            game.world.set(2, y, 0, stone).unwrap();
        }
        let stand = game.entity(STAND).unwrap();
        // Open air: the whole way.
        assert_eq!(
            game.entity_move_by(stand, [0.0, 0.0, 1.0], MoveCause::SelfMovement),
            Some([0.0, 0.0, 1.0])
        );
        assert_eq!(game.armor_stands[0].position.z, 1.5);
        // Towards the wall at x = 2: the stand's half width (0.25) stops it at x = 1.75.
        let moved = game
            .entity_move_by(stand, [3.0, 0.0, 0.0], MoveCause::SelfMovement)
            .unwrap();
        assert!((moved[0] - 1.0).abs() < 1.0e-6 && moved[1] == 0.0 && moved[2] == 0.0);
        assert!((game.armor_stands[0].position.x - 1.5).abs() < 1.0e-6);
        // Bad deltas, hanging entities and a stale handle.
        assert_eq!(
            game.entity_move_by(stand, [f64::NAN; 3], MoveCause::Piston),
            None
        );
        assert_eq!(
            game.entity_move_by(stand, [65.0, 0.0, 0.0], MoveCause::Piston),
            None
        );
        let frame = game.entity(FRAME).unwrap();
        assert_eq!(
            game.entity_move_by(frame, [1.0, 0.0, 0.0], MoveCause::SelfMovement),
            None
        );
        let gone = mod_api::EntityRef::new(
            mod_api::WorldEpoch::new(stand.epoch().get() + 1),
            stand.id(),
        );
        assert_eq!(
            game.entity_move_by(gone, [1.0, 0.0, 0.0], MoveCause::SelfMovement),
            None
        );
    });
}
