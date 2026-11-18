//! `LevelRef` placement and effects from code: `place_block`, `update_from_neighbour_shapes` and
//! `block_break_effect`.

use super::native_entity_refs::tests::with_game;
use mod_api::BlockUpdateFlags;

fn loaded(game: &mut super::Game<'_>) {
    // The writes are `level_set_block`'s, which an authoritative server only takes.
    game.options.dedicated = Some(Vec::new());
    game.world.chunks.insert(
        (0, 0),
        std::sync::Arc::new(sunrise_client::chunk::Chunk::empty(0, 0)),
    );
}

fn state(game: &super::Game<'_>, name: &str) -> sunrise_world::registry::BlockStateId {
    game.pack.assets.registry.default_state(name).unwrap()
}

/// A bed placed with no player gets its head (`setPlacedBy`), and placing it again changes nothing.
#[test]
#[ignore = "daily"]
fn a_block_placed_without_a_player_runs_set_placed_by() {
    with_game(|game| {
        loaded(game);
        let level = game.level(game.dim.id()).unwrap();
        let bed = state(game, "minecraft:red_bed");
        assert_eq!(
            game.level_place_block(level, [2, 70, 2], bed, true),
            Ok(true)
        );
        assert_eq!(game.world.block(2, 70, 2), bed);
        let head = game.world.block(2, 70, 1);
        let facts = game.res.facts();
        assert_eq!(
            facts.get(head).name,
            facts.get(bed).name,
            "the head was placed"
        );
        assert_eq!(
            game.level_place_block(level, [2, 70, 2], bed, true),
            Ok(false)
        );
    });
}

/// A fence is fitted to the fence beside it without anything being written.
#[test]
#[ignore = "daily"]
fn a_state_is_fitted_to_its_neighbours_without_a_write() {
    with_game(|game| {
        loaded(game);
        let level = game.level(game.dim.id()).unwrap();
        let fence = state(game, "minecraft:oak_fence");
        let air = game.world.block(2, 70, 2);
        assert_eq!(
            game.level_set_block(level, [3, 70, 2], fence, BlockUpdateFlags::NONE),
            Ok(true)
        );
        let fitted = game
            .level_update_from_neighbour_shapes(level, [2, 70, 2], fence)
            .unwrap();
        let sides = game.pack.assets.registry.state(fitted).unwrap();
        let side = |name: &str| sides.properties.get(name).map(String::as_str);
        assert_eq!(
            side("east"),
            Some("true"),
            "it connects to the fence at its east"
        );
        for other in ["north", "south", "west"] {
            assert_eq!(side(other), Some("false"), "nothing stands on its {other}");
        }
        assert_eq!(game.world.block(2, 70, 2), air, "nothing written");
        assert_eq!(
            game.level_update_from_neighbour_shapes(level, [9, 70, 9], fence),
            Ok(fence),
            "no neighbour, nothing to fit"
        );
    });
}

/// The break effect is level event 2001 with the state's id, and a stale handle plays nothing. The
/// event plays at once (`Level.levelEvent`), so what is observed is what reaches the guests.
#[test]
#[ignore = "daily"]
fn a_break_effect_is_played_for_a_state() {
    with_game(|game| {
        loaded(game);
        game.audio.capture = Some(Vec::new());
        let level = game.level(game.dim.id()).unwrap();
        let stone = state(game, "minecraft:stone");
        game.lan_events.clear();
        assert!(game.level_block_break_effect(level, [1, 70, 1], stone));
        assert!(
            game.lan_events
                .iter()
                .any(|e| e.kind == 2001 && e.pos == [1, 70, 1] && e.data == stone.0 as i32)
        );
        let stale = mod_api::LevelRef::new(
            mod_api::WorldEpoch::new(level.epoch().get() + 1),
            level.dimension(),
        );
        game.lan_events.clear();
        assert!(!game.level_block_break_effect(stale, [1, 70, 1], stone));
        assert!(game.lan_events.is_empty());
    });
}
