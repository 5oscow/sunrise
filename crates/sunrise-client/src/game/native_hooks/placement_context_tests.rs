#![cfg(test)]

use crate::game::native_entity_refs::tests::with_game;
use mod_api::{Hooks, ItemPart, LevelSide, MustSurviveEvent, PlacementContextEvent, Registrar};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use sunrise_core::{
    geometry::{Direction, Vec3},
    position::BlockPos,
};
use sunrise_world::level::PlaceContext;

struct Move;
#[mod_api::sunrise_part]
impl ItemPart for Move {
    fn placement_context(
        &self,
        _: LevelSide<'_>,
        event: &mut PlacementContextEvent<'_>,
    ) -> Option<Option<mod_api::PlacementContext>> {
        let mut context = event.context.clone();
        context.pos.y -= 2;
        context.face = Direction::Down;
        Some(Some(context))
    }
}
struct Refuse;
#[mod_api::sunrise_part]
impl ItemPart for Refuse {
    fn placement_context(
        &self,
        _: LevelSide<'_>,
        _: &mut PlacementContextEvent<'_>,
    ) -> Option<Option<mod_api::PlacementContext>> {
        Some(None)
    }
}
struct Invalid;
#[mod_api::sunrise_part]
impl ItemPart for Invalid {
    fn placement_context(
        &self,
        _: LevelSide<'_>,
        event: &mut PlacementContextEvent<'_>,
    ) -> Option<Option<mod_api::PlacementContext>> {
        let mut context = event.context.clone();
        context.hit[0] = f64::NAN;
        Some(Some(context))
    }
}
enum Survival {
    Skip,
    RequireWhen(Arc<AtomicBool>),
}
#[mod_api::sunrise_part]
impl ItemPart for Survival {
    fn must_survive(&self, _: LevelSide<'_>, _: &mut MustSurviveEvent<'_>) -> Option<bool> {
        match self {
            Self::Skip => Some(false),
            Self::RequireWhen(required) => required.load(Ordering::Relaxed).then_some(true),
        }
    }
}
struct Parts(Arc<AtomicBool>);
#[mod_api::__subscriptions]
impl Hooks for Parts {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        let stone = r.item_key("minecraft:stone");
        r.item_part(stone, Move);
        let dirt = r.item_key("minecraft:dirt");
        r.item_part(dirt, Refuse);
        let sand = r.item_key("minecraft:sand");
        r.item_part(sand, Invalid);
        let poppy = r.item_key("minecraft:poppy");
        r.item_part(poppy, Survival::Skip);
        let torch = r.item_key("minecraft:torch");
        r.item_part(torch, Survival::Skip);
        let scaffolding = r.item_key("minecraft:scaffolding");
        r.item_part(scaffolding, Survival::RequireWhen(self.0.clone()));
    }
}

#[test]
#[ignore = "daily"]
fn item_parts_rewrite_or_refuse_context_and_control_survival_on_both_sides() {
    let require_scaffolding_survival = Arc::new(AtomicBool::new(false));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "placement",
        Box::new(Parts(require_scaffolding_survival.clone())),
    )]
    .into();
    crate::game::api_values::install_parts(&mods).unwrap();
    mod_platform::testing::with_mods(mods, || {
        with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(sunrise_client::chunk::Chunk::empty(0, 0)),
            );
            let registry = game.pack.assets.registry;
            let air = registry.default_state("minecraft:air").unwrap();
            let scaffolding = registry
                .resolve(&sunrise_world::chunk::BlockState {
                    name: "minecraft:scaffolding".into(),
                    properties: [
                        ("bottom", "true"),
                        ("distance", "6"),
                        ("waterlogged", "false"),
                    ]
                    .into_iter()
                    .map(|(key, value)| (key.into(), value.into()))
                    .collect(),
                })
                .unwrap();
            for authoritative in [true, false] {
                game.with_level(|b, level, ticks| {
                    level.native_authoritative = authoritative;
                    level.client_side = !authoritative;
                    let context = |item: &str| PlaceContext {
                        clicked_pos: BlockPos::new(4, 64, 4),
                        face: Direction::Up,
                        hit: Vec3::new(4.5, 64.0, 4.5),
                        inside: false,
                        yaw: 0.0,
                        pitch: 0.0,
                        sneaking: false,
                        item: item.into(),
                        can_use_game_master_blocks: false,
                    };
                    let placed = b
                        .placement_preview(level, ticks, &context("minecraft:stone"))
                        .unwrap();
                    assert_eq!(placed.pos, BlockPos::new(4, 62, 4));
                    let fallback = b
                        .placement_preview(level, ticks, &context("minecraft:glass"))
                        .unwrap();
                    assert_eq!(fallback.pos, BlockPos::new(4, 64, 4));
                    assert!(
                        b.placement_preview(level, ticks, &context("minecraft:dirt"))
                            .is_none()
                    );
                    assert!(
                        b.placement_preview(level, ticks, &context("minecraft:sand"))
                            .is_none()
                    );
                    assert!(
                        b.placement_preview(level, ticks, &context("minecraft:poppy"))
                            .is_some(),
                        "survival can be skipped, even with no soil"
                    );
                    assert!(
                        b.placement_preview(level, ticks, &context("minecraft:torch"))
                            .is_none(),
                        "standing/wall candidates must survive even when final survival is skipped"
                    );
                    // Vanilla checks replaceability before the rewrite, not after it.
                    b.set_block(level, ticks, BlockPos::new(4, 62, 4), fallback.state, 3);
                    assert!(
                        b.placement_preview(level, ticks, &context("minecraft:stone"))
                            .is_some()
                    );
                    b.set_block(level, ticks, BlockPos::new(4, 62, 4), air, 3);
                });
            }

            // Click the end of a scaffold extending six blocks from support.
            // Its horizontal walk reaches distance seven, where only vanilla's
            // skipped survival check lets the item place.
            game.world.set(4, 64, 4, scaffolding).unwrap();
            game.with_level(|b, level, ticks| {
                let context = PlaceContext {
                    clicked_pos: BlockPos::new(4, 64, 4),
                    face: Direction::Up,
                    hit: Vec3::new(4.5, 65.0, 4.5),
                    inside: false,
                    yaw: 0.0,
                    pitch: 0.0,
                    sneaking: false,
                    item: "minecraft:scaffolding".into(),
                    can_use_game_master_blocks: false,
                };
                assert_eq!(
                    b.placement_preview(level, ticks, &context).unwrap().pos,
                    BlockPos::new(4, 64, 5),
                );
                require_scaffolding_survival.store(true, Ordering::Relaxed);
                assert!(b.placement_preview(level, ticks, &context).is_none());
            });

            // The connected client actually predicts through DryLevel.
            game.options.execution_role = crate::game::execution_role::ExecutionRole::Client;
            game.options.dedicated = None;
            let (pipe, _server) = sunrise_protocol::connection::Pipe::pair();
            game.connect_local(pipe, "PlacementPrediction").unwrap();
            game.world
                .chunks
                .insert((0, 0), Arc::new(sunrise_client::chunk::Chunk::empty(0, 0)));
            game.sync_tick_chunks();
            game.set_mode_id(0);
            let hit = crate::game::Hit {
                pos: [4, 64, 4],
                face: [0, 1, 0],
                point: [4.5, 65.0, 4.5],
                inside: false,
            };
            let stack = |item| sunrise_client::items::ItemStack::new(item, 1);
            let (pos, changes) = game
                .remote_can_place(&hit, &stack("minecraft:stone"))
                .expect("rewritten stone placement");
            assert_eq!(pos, BlockPos::new(4, 62, 4));
            assert!(changes.contains(&(pos, registry.default_state("minecraft:stone").unwrap())));
            assert_eq!(game.world.block(pos.x, pos.y, pos.z), air);
            assert!(
                game.remote_can_place(&hit, &stack("minecraft:dirt"))
                    .is_none()
            );
            assert!(
                game.remote_can_place(&hit, &stack("minecraft:poppy"))
                    .is_some()
            );
        })
    });
}
