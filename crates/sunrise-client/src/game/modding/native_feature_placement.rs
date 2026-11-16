//! World-generation features placed for server mods: `/place feature`'s path,
//! for a feature or a placed feature the game has by id.
use super::Game;
use sunrise_core::position::BlockPos;

/// Native mods use this; nothing in Sunrise does.
impl Game<'_> {
    /// For a server mod: the feature `id` (an entry of `minecraft:worldgen/feature`)
    /// placed at `at` as `/place feature` places it. `Ok(false)` is the feature's
    /// own answer that it placed nothing, and may still have written blocks.
    /// API @API:place_feature@.
    pub(super) fn place_feature(&mut self, id: &str, at: [i32; 3]) -> Result<bool, String> {
        self.place_feature_by_id("feature", id, at, false)
    }

    /// For a server mod: the placed feature `id` (an entry of
    /// `minecraft:worldgen/placed_feature`) placed at `at`: its placement
    /// modifiers from `at`, then its feature at every position they give.
    /// `Ok(false)` as for `place_feature`.
    /// API @API:place_feature@.
    pub(super) fn place_placed_feature(&mut self, id: &str, at: [i32; 3]) -> Result<bool, String> {
        self.place_feature_by_id("placed feature", id, at, true)
    }

    /// `PlaceCommand.checkLoaded` over the 3 by 3 chunks around `at`, then the
    /// placement the command takes; `Err` says why nothing was placed.
    fn place_feature_by_id(
        &mut self,
        kind: &str,
        id: &str,
        at: [i32; 3],
        placed: bool,
    ) -> Result<bool, String> {
        if !self.native_authoritative() {
            return Err(
                "a client of a server places nothing: the level's own server places a feature"
                    .into(),
            );
        }
        let (cx, cz) = (at[0] >> 4, at[2] >> 4);
        for x in cx - 1..=cx + 1 {
            for z in cz - 1..=cz + 1 {
                if !self.world.chunks.contains_key(&(x, z)) {
                    return Err(format!(
                        "chunk ({x}, {z}) around the origin is not loaded, so nothing was placed"
                    ));
                }
            }
        }
        self.place_named_feature(id, BlockPos::new(at[0], at[1], at[2]), placed)
            .ok_or_else(|| {
                format!(
                    "the game has no {kind} `{id}`, or no feature resources, so nothing was placed"
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use sunrise_client::chunk::Chunk;

    fn ready(game: &mut Game<'_>, chunks: &[(i32, i32)]) {
        crate::game::growth::install_live_features(
            &game.res,
            &game.pack,
            &game.options,
            &Default::default(),
        );
        for &(x, z) in chunks {
            game.world
                .chunks
                .insert((x, z), Arc::new(Chunk::empty(x, z)));
        }
        // The shared fixture is unticked: a dedicated server is what a server
        // host call asks for.
        game.options.dedicated = Some(Vec::new());
    }

    fn around(x: i32, z: i32) -> Vec<(i32, i32)> {
        (x - 1..=x + 1)
            .flat_map(|cx| (z - 1..=z + 1).map(move |cz| (cx, cz)))
            .collect()
    }

    #[test]
    #[ignore = "daily"]
    fn a_feature_places_and_a_placed_feature_runs_its_modifiers_first() {
        use crate::game::native_entity_refs::tests::with_game;

        with_game(|game| {
            ready(game, &around(0, 0));
            let registry = game.pack.assets.registry;
            let state = |name: &str| registry.default_state(name).unwrap();
            let (dirt, stone, log, air) = (
                state("minecraft:dirt"),
                state("minecraft:stone"),
                state("minecraft:oak_log"),
                state("minecraft:air"),
            );
            // A tree's trunk starts at its origin, on soil.
            game.world.set(4, 64, 4, dirt).unwrap();
            assert_eq!(game.place_feature("minecraft:oak", [4, 65, 4]), Ok(true));
            assert_eq!(game.world.block(4, 65, 4), log, "the trunk's base");
            // `oak_checked` is `oak` behind a `would_survive` filter for an oak
            // sapling: no soil gives no position, so the feature never runs.
            game.world.set(20, 64, 20, stone).unwrap();
            assert_eq!(
                game.place_placed_feature("minecraft:oak_checked", [20, 65, 20]),
                Ok(false)
            );
            assert_eq!(game.world.block(20, 65, 20), air);
            game.world.set(20, 64, 4, dirt).unwrap();
            assert_eq!(
                game.place_placed_feature("minecraft:oak_checked", [20, 65, 4]),
                Ok(true)
            );
            assert_eq!(game.world.block(20, 65, 4), log);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn an_unknown_id_a_client_and_an_unloaded_chunk_refuse_and_place_nothing() {
        use crate::game::native_entity_refs::tests::with_game;

        with_game(|game| {
            // Chunk (0, 0)'s ring misses (-1, 1).
            let mut chunks = around(0, 0);
            chunks.retain(|&c| c != (-1, 1));
            ready(game, &chunks);
            let unknown = game.place_feature("mymod:nothing", [8, 65, 8]);
            assert!(
                unknown.unwrap_err().contains("no feature `mymod:nothing`"),
                "an id the registry lacks"
            );
            let unknown = game.place_placed_feature("mymod:nothing", [8, 65, 8]);
            assert!(unknown.unwrap_err().contains("no placed feature"));
            let unloaded = game.place_feature("minecraft:oak", [8, 65, 8]);
            assert!(
                unloaded
                    .unwrap_err()
                    .starts_with("chunk (-1, 1) around the origin is not loaded"),
            );
            game.options.dedicated = None;
            let client = game.place_feature("minecraft:oak", [8, 65, 8]);
            assert!(client.unwrap_err().starts_with("a client of a server"));
        });
    }
}
