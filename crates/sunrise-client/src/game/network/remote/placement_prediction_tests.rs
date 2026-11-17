use super::*;
use crate::game::native_entity_refs::tests::with_game_role;
use mod_api::{
    BlockPart, BlockState, CanSurviveEvent, CanSustainPlantEvent, Hooks, LevelSide,
    MayPlaceOnEvent, Registrar, StateForPlacementEvent,
};

/// Every answer below is a client's: the hooks are asked by its placement prediction.
fn client(level: LevelSide<'_>) {
    assert!(matches!(level, LevelSide::Client(_)));
}

struct Refuse;
#[mod_api::sunrise_part]
impl BlockPart for Refuse {
    fn state_for_placement(
        &self,
        level: LevelSide<'_>,
        _: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        client(level);
        Some(None)
    }
}

struct Fall;
#[mod_api::sunrise_part]
impl BlockPart for Fall {
    fn can_survive(&self, level: LevelSide<'_>, _: &mut CanSurviveEvent) -> Option<bool> {
        client(level);
        Some(false)
    }
}

struct Soil;
#[mod_api::sunrise_part]
impl BlockPart for Soil {
    fn can_sustain_plant(
        &self,
        level: LevelSide<'_>,
        _: &mut CanSustainPlantEvent,
    ) -> Option<bool> {
        client(level);
        Some(true)
    }
}

struct Hardy;
#[mod_api::sunrise_part]
impl BlockPart for Hardy {
    fn may_place_on(&self, level: LevelSide<'_>, _: &mut MayPlaceOnEvent) -> Option<bool> {
        client(level);
        Some(true)
    }
}

struct PlaceMod;
#[mod_api::__subscriptions]
impl Hooks for PlaceMod {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        let dirt = r.block_key("minecraft:dirt");
        r.block_part(dirt, Refuse);
        let cobblestone = r.block_key("minecraft:cobblestone");
        r.block_part(cobblestone, Fall);
        let glass = r.block_key("minecraft:glass");
        r.block_part(glass, Soil);
        let birch = r.block_key("minecraft:birch_sapling");
        r.block_part(birch, Hardy);
    }
}

#[test]
#[ignore = "daily"]
fn client_placement_prediction_asks_placement_and_survival_parts() {
    let root = std::path::PathBuf::from(std::env::var_os("SUNRISE_TEST_RESOURCES").unwrap());
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new("place_test", Box::new(PlaceMod))].into();
    crate::game::api_values::install_parts(&mods).unwrap();
    mod_platform::testing::with_mods(mods, || {
        with_game_role(
            root,
            crate::game::execution_role::ExecutionRole::Client,
            |game| {
                let (pipe, _server) = sunrise_protocol::connection::Pipe::pair();
                game.connect_local(pipe, "PlacePrediction").unwrap();
                game.world
                    .chunks
                    .insert((0, 0), Arc::new(sunrise_client::chunk::Chunk::empty(0, 0)));
                game.sync_tick_chunks();
                game.set_mode_id(0);
                // (ground, held block, whether the client predicts the placement)
                for (i, (ground, held, placed)) in [
                    ("minecraft:stone", "minecraft:dirt", false),
                    ("minecraft:stone", "minecraft:cobblestone", false),
                    ("minecraft:stone", "minecraft:oak_sapling", false),
                    ("minecraft:glass", "minecraft:oak_sapling", true),
                    ("minecraft:stone", "minecraft:birch_sapling", true),
                ]
                .into_iter()
                .enumerate()
                {
                    let x = i as i32 * 2;
                    let below = game.pack.assets.registry.default_state(ground).unwrap();
                    game.world.set(x, 64, 0, below).unwrap();
                    let hit = crate::game::Hit {
                        pos: [x, 64, 0],
                        face: [0, 1, 0],
                        point: [f64::from(x) + 0.5, 65.0, 0.5],
                        inside: false,
                    };
                    let preview = game.remote_can_place(&hit, &ItemStack::new(held, 1));
                    assert_eq!(
                        preview.map(|(pos, _)| pos),
                        placed.then(|| sunrise_core::position::BlockPos::new(x, 65, 0)),
                        "{held} on {ground}"
                    );
                }
            },
        );
    });
}
