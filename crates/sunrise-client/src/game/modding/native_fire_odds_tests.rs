//! `fire_odds` answered by a block Part with the asked block's position and face (#3185): the
//! Part's answer for one face makes the fire beside that face valid, and the other faces keep
//! vanilla's table.
use super::native_entity_refs::tests::{with_client_game, with_game};
use mod_api::{self, BlockPart, FireOddsEvent, Hooks, LevelSide, Registrar};
use sunrise_core::{geometry::Direction, position::BlockPos};

/// Glass that ignites from its west face only.
struct WestFace {
    client: bool,
}

#[mod_api::sunrise_part]
impl BlockPart for WestFace {
    fn fire_odds(&self, host: LevelSide<'_>, event: &mut FireOddsEvent) -> Option<(i32, i32)> {
        assert_eq!(matches!(host, LevelSide::Client(_)), self.client);
        (event.face == Direction::West).then_some((5, 0))
    }
}

struct FireOddsMod;

#[mod_api::__subscriptions]
impl Hooks for FireOddsMod {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        let glass = r.block_key("minecraft:glass");
        r.block_part(glass, WestFace { client: false });
        let tinted_glass = r.block_key("minecraft:tinted_glass");
        r.block_part(tinted_glass, WestFace { client: true });
    }
}

#[test]
#[ignore = "daily"]
fn a_block_part_answers_fire_odds_for_one_face() {
    use std::sync::Arc;
    use sunrise_client::{chunk::Chunk, items::ItemStack};

    let mods: mod_api::Mods =
        vec![mod_api::ModEntry::new("fire_odds", Box::new(FireOddsMod))].into();
    mod_platform::testing::with_mods(mods.clone(), || {
        crate::game::api_values::install_parts(&mods).unwrap();
        with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let registry = game.pack.assets.registry;
            let glass = registry.default_state("minecraft:glass").unwrap();
            let fire = registry.default_state("minecraft:fire").unwrap();
            game.world.set(1, 65, 0, glass).unwrap();
            // Fire in the air at `x`, beside the glass, with air below it: it
            // stands only beside a flammable face.
            let stands = |game: &mut super::Game<'_>, x: i32| {
                game.with_level(|b, level, ticks| {
                    b.can_survive(level, ticks, BlockPos::new(x, 65, 0), fire)
                })
            };
            assert!(stands(game, 0), "the Part makes the west face flammable");
            assert!(!stands(game, 2), "the east face keeps vanilla's never");
        });
        with_client_game(|game| {
            let (pipe, _server) = sunrise_protocol::connection::Pipe::pair();
            game.connect_local(pipe, "FireOddsPrediction").unwrap();
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            game.sync_tick_chunks();
            game.set_mode_id(0);

            let registry = game.pack.assets.registry;
            let glass = registry.default_state("minecraft:tinted_glass").unwrap();
            let fire = registry.default_state("minecraft:fire").unwrap();
            let stone = registry.default_state("minecraft:stone").unwrap();
            let air = game.res.facts().air;
            // Putting stone beside the fire forces a dry neighbour shape update.
            // The glass face is the only possible support for the fire.
            for (x, glass_x, stands) in [(2, 3, true), (6, 5, false)] {
                game.world.set(glass_x, 65, 2, glass).unwrap();
                game.world.set(x, 65, 2, fire).unwrap();
                game.world.set(x, 64, 3, stone).unwrap();
                let hit = crate::game::Hit {
                    pos: [x, 64, 3],
                    face: [0, 1, 0],
                    point: [f64::from(x) + 0.5, 65.0, 3.5],
                    inside: false,
                };
                let (placed, changes) = game
                    .remote_can_place(&hit, &ItemStack::new("minecraft:stone", 1))
                    .expect("stone placement prediction");
                assert_eq!(placed, BlockPos::new(x, 65, 3));
                let pos = BlockPos::new(x, 65, 2);
                let predicted = changes
                    .iter()
                    .find(|(at, _)| *at == pos)
                    .map_or(fire, |(_, state)| *state);
                if stands {
                    assert_eq!(registry.state(predicted).unwrap().name, "minecraft:fire");
                } else {
                    assert_eq!(predicted, air, "non-flammable face loses the fire");
                }
                assert_eq!(game.world.block(x, 65, 2), fire, "prediction stays dry");
            }
        });
    });
}
