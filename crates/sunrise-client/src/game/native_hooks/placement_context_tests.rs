#![cfg(test)]

use crate::game::native_entity_refs::tests::with_game;
use mod_api::{Hooks, ItemPart, LevelSide, MustSurviveEvent, PlacementContextEvent, Registrar};
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
struct Survival(bool);
#[mod_api::sunrise_part]
impl ItemPart for Survival {
    fn must_survive(&self, _: LevelSide<'_>, _: &mut MustSurviveEvent<'_>) -> Option<bool> {
        Some(self.0)
    }
}
struct Parts;
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
        r.item_part(poppy, Survival(false));
        let scaffolding = r.item_key("minecraft:scaffolding");
        r.item_part(scaffolding, Survival(true));
    }
}

#[test]
#[ignore = "daily"]
fn item_parts_rewrite_or_refuse_context_and_control_survival_on_both_sides() {
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new("placement", Box::new(Parts))].into();
    crate::game::api_values::install_parts(&mods).unwrap();
    mod_platform::testing::with_mods(mods, || {
        with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(sunrise_client::chunk::Chunk::empty(0, 0)),
            );
            let air = game
                .pack
                .assets
                .registry
                .default_state("minecraft:air")
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
                        b.placement_preview(level, ticks, &context("minecraft:scaffolding"))
                            .is_none(),
                        "survival can be required where vanilla skips it"
                    );
                    // A rewritten target cannot overwrite an occupied cell.
                    b.set_block(level, ticks, BlockPos::new(4, 62, 4), fallback.state, 3);
                    assert!(
                        b.placement_preview(level, ticks, &context("minecraft:stone"))
                            .is_none()
                    );
                    b.set_block(level, ticks, BlockPos::new(4, 62, 4), air, 3);
                });
            }
        })
    });
}
