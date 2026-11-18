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

/// `JukeboxBlock.setPlacedBy` reads the supplied stack's RecordItem key but does
/// not load its NBT into the jukebox; only the block state reflects that key.
#[test]
#[ignore = "daily"]
fn a_post_write_placement_reads_the_stack_without_loading_its_data() {
    use crate::game::api_values::TryIntoApi;
    use sunrise_client::{block_entity_item::BLOCK_ENTITY_DATA, items::ItemStack};

    with_game(|game| {
        let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
        loaded(game);
        let level = game.level(game.dim.id()).unwrap();
        let pos = [2, 70, 2];
        let jukebox = state(game, "minecraft:jukebox");
        assert_eq!(
            game.level_set_block(level, pos, jukebox, BlockUpdateFlags::NONE),
            Ok(true)
        );
        assert_eq!(level.set_placed_by(game, pos, None), Ok(true));
        assert_eq!(game.world.block(pos[0], pos[1], pos[2]), jukebox);

        let mut stack = ItemStack::new("minecraft:jukebox", 1);
        let data = sunrise_client::command::snbt::parse_fully(
            r#"{id:"minecraft:jukebox",RecordItem:{id:"minecraft:music_disc_cat",count:1}}"#,
        )
        .unwrap();
        stack.data_mut().set_other(BLOCK_ENTITY_DATA, Some(data));
        let mut stack = stack.try_into_api().unwrap();
        stack.set_count(100).unwrap();
        stack.set_damage(12);
        assert_eq!(
            level.set_placed_by(game, [128, 70, 2], Some(&stack)),
            Ok(false)
        );
        assert_eq!(level.set_placed_by(game, pos, Some(&stack)), Ok(true));
        let placed = game
            .pack
            .assets
            .registry
            .state(game.world.block(pos[0], pos[1], pos[2]))
            .unwrap();
        assert_eq!(
            placed.properties.get("has_record").map(String::as_str),
            Some("true")
        );
        assert!(
            game.block_entity_nbt(pos)
                .unwrap()
                .get("RecordItem")
                .is_none()
        );
    });
}

#[test]
#[ignore = "daily"]
fn placement_does_not_write_a_fenced_companion() {
    with_game(|game| {
        let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
        loaded(game);
        let level = game.level(game.dim.id()).unwrap();
        let foot = [2, 70, 2];
        let head = [2, 70, 1];
        let bed = state(game, "minecraft:red_bed");
        let stone = state(game, "minecraft:stone");
        assert_eq!(
            game.level_set_block(level, head, stone, BlockUpdateFlags::NONE),
            Ok(true)
        );
        game.world.carrier_fences.lock_cell(head);
        assert_eq!(game.level_place_block(level, foot, bed, false), Ok(false));
        assert_eq!(game.world.block(2, 70, 2), game.world.air);
        assert_eq!(
            game.level_set_block(level, foot, bed, BlockUpdateFlags::NONE),
            Ok(true)
        );
        assert_eq!(level.set_placed_by(game, foot, None), Ok(false));
        assert_eq!(game.world.block(2, 70, 1), stone);
        assert_eq!(game.world.block(2, 70, 2), bed);
    });
}

#[test]
#[ignore = "daily"]
fn a_wither_placement_refused_in_a_transaction_keeps_the_pattern() {
    use mod_api::{HostTransaction, LevelWriteError, ServerHost};

    with_game(|game| {
        let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
        loaded(game);
        game.ui.difficulty = 2;
        let level = game.level(game.dim.id()).unwrap();
        let skull = state(game, "minecraft:wither_skeleton_skull");
        let soul = state(game, "minecraft:soul_sand");
        let air = game.world.air;
        let cells = [
            ([4, 72, 4], skull),
            ([5, 72, 4], skull),
            ([6, 72, 4], skull),
            ([4, 71, 4], soul),
            ([5, 71, 4], soul),
            ([6, 71, 4], soul),
            ([4, 70, 4], air),
            ([5, 70, 4], soul),
            ([6, 70, 4], air),
        ];
        for (pos, state) in cells.into_iter().filter(|(_, state)| *state != air) {
            assert_eq!(
                game.level_set_block(level, pos, state, BlockUpdateFlags::NONE),
                Ok(true)
            );
        }
        let at = [5, 72, 4];
        let before = game.mobs.len();
        let aborted: Result<(), ()> = (game as &mut dyn ServerHost).transaction(|host, _| {
            assert_eq!(
                level.set_placed_by(host, at, None),
                Err(LevelWriteError::InTransaction)
            );
            Err(())
        });
        assert_eq!(aborted, Err(()));
        for ([x, y, z], state) in cells {
            assert_eq!(game.world.block(x, y, z), state);
        }
        assert_eq!(game.mobs.len(), before);
        assert_eq!(level.set_placed_by(game, at, None), Ok(true));
        assert_eq!(game.mobs.len(), before + 1);
        for ([x, y, z], _) in cells {
            assert_eq!(game.world.block(x, y, z), air);
        }
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
