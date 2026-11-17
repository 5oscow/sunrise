//! Vegetation is a Part behaviour, not a restriction on a block's visual template.
use mod_api::{BlockPart, Hooks, LevelHost, MayPlaceOnEvent, Registrar};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use sunrise_core::{
    geometry::{Direction, Vec3},
    position::{BlockPos, ChunkPos},
};
use sunrise_world::level::{PlaceContext, PlaceOutcome};

struct Plant(Arc<AtomicUsize>);
#[mod_api::sunrise_part]
impl BlockPart for Plant {
    fn may_place_on(&self, _: &dyn LevelHost, _: &mut MayPlaceOnEvent) -> Option<bool> {
        self.0.fetch_add(1, Ordering::Relaxed);
        None // Declare vegetation, using its default supports_vegetation tag.
    }
}
struct Soil(bool);
#[mod_api::sunrise_part]
impl BlockPart for Soil {
    fn can_sustain_plant(
        &self,
        _: &dyn LevelHost,
        _: &mut mod_api::CanSustainPlantEvent,
    ) -> Option<bool> {
        Some(self.0)
    }
}
struct Garden(Arc<AtomicUsize>);
const PLANTS: [(&str, &str); 3] = [
    ("garden:glass", "minecraft:glass"),
    ("garden:stairs", "minecraft:oak_stairs"),
    ("garden:poppy", "minecraft:poppy"),
];
#[mod_api::__subscriptions]
impl Hooks for Garden {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        for (id, _) in PLANTS {
            let key = r.block_key(id);
            r.block_part(key, Plant(Arc::clone(&self.0)));
        }
        for (id, answer) in [
            ("minecraft:gold_block", true),
            ("minecraft:grass_block", false),
        ] {
            let key = r.block_key(id);
            r.block_part(key, Soil(answer));
        }
    }
    fn blocks(&self, _: &mod_api::RegistrationContext, event: &mut mod_api::BlocksEvent<'_>) {
        for (id, template) in PLANTS
            .into_iter()
            .chain([("garden:ordinary_glass", "minecraft:glass")])
        {
            event.blocks.push(mod_api::Block::new(id, template));
        }
    }
}

#[test]
#[ignore = "daily"]
fn declared_vegetation_places_and_breaks_independently_of_its_template() {
    if super::native_entity_refs::tests::run_fresh_cases(
        "game::declared_vegetation_tests::declared_vegetation_places_and_breaks_independently_of_its_template",
        "SUNRISE_VEGETATION_CHILD",
        &["run"],
    ) {
        return;
    }
    let heard = Arc::new(AtomicUsize::new(0));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "garden",
        Box::new(Garden(Arc::clone(&heard))),
    )]
    .into();
    mod_platform::testing::with_mods(mods.clone(), || {
        crate::game::api_values::install_parts(&mods).unwrap();
        mod_api::__private::mark_game_thread();
        let base = std::path::PathBuf::from(std::env::var_os("SUNRISE_TEST_RESOURCES").unwrap());
        let root = super::mods::content_root(&base, &Default::default()).unwrap();
        super::native_entity_refs::tests::with_game_at(root.prepared().to_path_buf(), |game| {
            game.options.dedicated = Some(Vec::new());
            game.world
                .chunks
                .insert((0, 0), Arc::new(sunrise_client::chunk::Chunk::empty(0, 0)));
            game.block_ticks
                .add_empty_chunk(ChunkPos::new(0, 0))
                .unwrap();
            let registry = game.pack.assets.registry;
            let air = registry.default_state("minecraft:air").unwrap();
            let stone = registry.default_state("minecraft:stone").unwrap();
            let pos = BlockPos::new(5, 65, 5);
            for (plant, _) in PLANTS {
                for (soil, allowed, asks_plant) in [
                    ("minecraft:stone", false, true),
                    ("minecraft:dirt", true, true),
                    ("minecraft:gold_block", true, false),
                    ("minecraft:grass_block", false, false),
                ] {
                    game.world.set(5, 65, 5, air).unwrap();
                    game.world
                        .set(5, 64, 5, registry.default_state(soil).unwrap())
                        .unwrap();
                    let context = PlaceContext {
                        clicked_pos: BlockPos::new(5, 64, 5),
                        face: Direction::Up,
                        hit: Vec3::new(5.5, 65.0, 5.5),
                        inside: false,
                        yaw: 0.0,
                        pitch: 0.0,
                        sneaking: false,
                        item: plant.into(),
                        can_use_game_master_blocks: false,
                    };
                    let before = heard.load(Ordering::Relaxed);
                    let placed =
                        game.with_level(|b, level, ticks| b.place_block(level, ticks, &context));
                    assert_eq!(
                        matches!(placed, PlaceOutcome::Placed { .. }),
                        allowed,
                        "{plant} on {soil}"
                    );
                    assert_eq!(
                        heard.load(Ordering::Relaxed) > before,
                        asks_plant,
                        "soil decides before the plant"
                    );
                    assert_eq!(game.world.block(5, 65, 5) != air, allowed);
                    if allowed {
                        game.with_level(|b, level, ticks| {
                            b.set_block(level, ticks, BlockPos::new(5, 64, 5), stone, 3);
                        });
                        assert_eq!(
                            game.world.block(pos.x, pos.y, pos.z),
                            air,
                            "{plant} pops off after its soil changes"
                        );
                    }
                }
            }
            // The same glass appearance without a vegetation Part stays an ordinary block.
            let ordinary = registry.default_state("garden:ordinary_glass").unwrap();
            game.world.set(5, 64, 5, stone).unwrap();
            assert!(game.with_level(|b, level, ticks| b.can_survive(level, ticks, pos, ordinary)));
        });
    });
}
