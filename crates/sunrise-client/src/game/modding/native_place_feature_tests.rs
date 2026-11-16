//! `level.place_feature` through the public handle: a tree placed as `/place feature` places it,
//! a feature with no room, and a literal test for each refusal.

use super::native_entity_refs::tests::{with_client_game, with_game};
use mod_api::{LevelRef, PlaceFeatureRefusal, WorldEpoch};
use std::sync::Arc;
use sunrise_client::chunk::Chunk;

/// The 3x3 chunks around (100, 100): a dirt floor at y 63 under open air, or, with `buried`,
/// stone from y 63 up to y 90.
fn ground(game: &mut super::Game<'_>, buried: bool) {
    // As a world opens, its live features are installed (`prepare_world`).
    game.install_live_features();
    let registry = &game.pack.assets.registry;
    let dirt = registry.default_state("minecraft:dirt").unwrap();
    let stone = registry.default_state("minecraft:stone").unwrap();
    for dz in -1..=1 {
        for dx in -1..=1 {
            let mut chunk = Chunk::empty(100 + dx, 100 + dz);
            for (x, z) in (0..16).flat_map(|x| (0..16).map(move |z| (x, z))) {
                chunk.set(x, 63, z, dirt.0);
                if buried {
                    for y in 63..=90 {
                        chunk.set(x, y, z, stone.0);
                    }
                }
            }
            game.world
                .chunks
                .insert((100 + dx, 100 + dz), Arc::new(chunk));
        }
    }
}

/// The middle of chunk (100, 100), one above the floor.
const AT: [i32; 3] = [1608, 64, 1608];

fn stale(level: LevelRef) -> LevelRef {
    LevelRef::new(WorldEpoch::new(level.epoch().get() + 1), level.dimension())
}

/// An oak placed on open ground puts its log at the position, as `/place feature minecraft:oak`.
#[test]
#[ignore = "daily"]
fn a_tree_is_placed_as_the_command_places_it() {
    with_game(|game| {
        ground(game, false);
        let level = game.level(game.dim.id()).unwrap();
        assert_eq!(level.place_feature(game, "minecraft:oak", AT), Ok(true));
        let log = game.world.block(AT[0], AT[1], AT[2]);
        assert_eq!(game.res.facts().get(log).name.as_ref(), "minecraft:oak_log");
    });
}

/// A tree buried in stone reports failure, `Ok(false)`, and this one set no block: the stone is
/// untouched.
#[test]
#[ignore = "daily"]
fn a_feature_with_no_room_places_nothing() {
    with_game(|game| {
        ground(game, true);
        let level = game.level(game.dim.id()).unwrap();
        let before = game.world.block(AT[0], AT[1], AT[2]);
        assert_eq!(level.place_feature(game, "minecraft:oak", AT), Ok(false));
        assert_eq!(game.world.block(AT[0], AT[1], AT[2]), before);
    });
}

/// An id that is not a feature, or has no namespace, is `UnknownFeature`.
#[test]
#[ignore = "daily"]
fn an_unknown_feature_is_refused_by_name() {
    with_game(|game| {
        ground(game, false);
        let level = game.level(game.dim.id()).unwrap();
        for id in ["mymod:no_such_tree", "oak", "minecraft:oak_log"] {
            assert_eq!(
                level.place_feature(game, id, AT),
                Err(PlaceFeatureRefusal::UnknownFeature),
                "{id}"
            );
        }
        assert_eq!(
            game.res
                .facts()
                .get(game.world.block(AT[0], AT[1], AT[2]))
                .name
                .as_ref(),
            "minecraft:air"
        );
    });
}

/// A position whose chunk ring is not all loaded, or out of height, is `NotLoaded`.
#[test]
#[ignore = "daily"]
fn unloaded_chunks_are_refused_by_name() {
    with_game(|game| {
        ground(game, false);
        let level = game.level(game.dim.id()).unwrap();
        // Chunk (101, 100) is loaded, but (102, 100) of its ring is not.
        assert_eq!(
            level.place_feature(game, "minecraft:oak", [1624, 64, 1608]),
            Err(PlaceFeatureRefusal::NotLoaded)
        );
        assert_eq!(
            level.place_feature(game, "minecraft:oak", [AT[0], 10_000, AT[2]]),
            Err(PlaceFeatureRefusal::NotLoaded)
        );
    });
}

/// A handle of an earlier open is `Stale`; a level open but not being ticked is `NotInPlace`.
#[test]
#[ignore = "daily"]
fn stale_and_other_levels_are_refused_by_name() {
    with_game(|game| {
        ground(game, false);
        let overworld = game.level("minecraft:overworld").unwrap();
        assert_eq!(
            stale(overworld).place_feature(game, "minecraft:oak", AT),
            Err(PlaceFeatureRefusal::Stale)
        );
        game.enter_level(super::Dim::Nether).unwrap();
        assert_eq!(
            overworld.place_feature(game, "minecraft:oak", AT),
            Err(PlaceFeatureRefusal::NotInPlace)
        );
    });
}

/// On a client it is `Client`.
#[test]
#[ignore = "daily"]
fn a_client_cannot_place_features() {
    with_client_game(|game| {
        ground(game, false);
        let level = LevelRef::new(WorldEpoch::new(0), game.dim.key());
        assert_eq!(
            level.place_feature(game, "minecraft:oak", AT),
            Err(PlaceFeatureRefusal::Client)
        );
    });
}

/// `oak_checked` is `oak` behind a `would_survive` filter for an oak sapling: on dirt it places, and
/// on stone the filter leaves no position, so it answers `Ok(false)` and writes nothing.
#[test]
#[ignore = "daily"]
fn a_placed_feature_runs_its_modifiers_before_the_feature() {
    with_game(|game| {
        ground(game, false);
        let level = game.level(game.dim.id()).unwrap();
        assert_eq!(
            level.place_placed_feature(game, "minecraft:oak_checked", AT),
            Ok(true)
        );
        let log = game.world.block(AT[0], AT[1], AT[2]);
        assert_eq!(game.res.facts().get(log).name.as_ref(), "minecraft:oak_log");
    });
    with_game(|game| {
        ground(game, true);
        let level = game.level(game.dim.id()).unwrap();
        let before = game.world.block(AT[0], AT[1], AT[2]);
        assert_eq!(
            level.place_placed_feature(game, "minecraft:oak_checked", AT),
            Ok(false)
        );
        assert_eq!(game.world.block(AT[0], AT[1], AT[2]), before);
    });
}

/// A biome filter admits birch trees in birch forest, but not in desert.
#[test]
#[ignore = "daily"]
fn a_biome_filtered_placed_feature_places_blocks_from_code() {
    for (biome, allowed) in [
        ("minecraft:birch_forest", true),
        ("minecraft:desert", false),
    ] {
        with_game(|game| {
            ground(game, false);
            let biome_id = world_biomes!(game)
                .list
                .iter()
                .position(|b| b.name == biome)
                .unwrap() as u16;
            for chunk in game.world.chunks.values_mut() {
                let chunk = Arc::make_mut(chunk);
                for qy in 0..(chunk.sections.len() as i32 * 4) {
                    for qx in 0..4 {
                        for qz in 0..4 {
                            chunk.set_biome_cell(qx, qy, qz, biome_id);
                        }
                    }
                }
            }
            let level = game.level(game.dim.id()).unwrap();
            assert_eq!(
                level.place_placed_feature(game, "minecraft:trees_birch", AT),
                Ok(allowed),
                "{biome}"
            );
            let mut logs = 0;
            let mut changed = 0;
            for x in 1600..1632 {
                for z in 1600..1632 {
                    for y in 64..90 {
                        let name = &game.res.facts().get(game.world.block(x, y, z)).name;
                        logs += usize::from(name.as_ref() == "minecraft:birch_log");
                        changed += usize::from(name.as_ref() != "minecraft:air");
                    }
                }
            }
            if allowed {
                assert!(logs > 0);
            } else {
                assert_eq!(changed, 0);
            }
        });
    }
}

/// A configured feature's id is not a placed feature's, and the other way round.
#[test]
#[ignore = "daily"]
fn a_placed_feature_id_is_checked_in_its_own_registry() {
    with_game(|game| {
        ground(game, false);
        let level = game.level(game.dim.id()).unwrap();
        for id in ["mymod:no_such_tree", "oak_checked", "minecraft:oak"] {
            assert_eq!(
                level.place_placed_feature(game, id, AT),
                Err(PlaceFeatureRefusal::UnknownFeature),
                "{id}"
            );
        }
        assert_eq!(
            level.place_feature(game, "minecraft:oak_checked", AT),
            Err(PlaceFeatureRefusal::UnknownFeature)
        );
    });
}
