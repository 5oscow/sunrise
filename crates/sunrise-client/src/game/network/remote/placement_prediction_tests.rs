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

/// `test` on a survival-mode client game with `mods`' parts, connected to a server and with
/// chunk (0, 0) loaded and empty.
fn on_a_client(mods: mod_api::Mods, test: impl FnOnce(&mut Game<'_>)) {
    let root = std::path::PathBuf::from(std::env::var_os("SUNRISE_TEST_RESOURCES").unwrap());
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
                test(game);
            },
        );
    });
}

/// The click at the top of `[x, 64, z]`.
fn top_of(x: i32, z: i32) -> crate::game::Hit {
    crate::game::Hit {
        pos: [x, 64, z],
        face: [0, 1, 0],
        point: [f64::from(x) + 0.5, 65.0, f64::from(z) + 0.5],
        inside: false,
    }
}

#[test]
#[ignore = "daily"]
fn client_placement_prediction_asks_placement_and_survival_parts() {
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new("place_test", Box::new(PlaceMod))].into();
    on_a_client(mods, |game| {
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
            let preview = game.remote_can_place(&top_of(x, 0), &ItemStack::new(held, 1));
            assert_eq!(
                preview.map(|(pos, _)| pos),
                placed.then(|| sunrise_core::position::BlockPos::new(x, 65, 0)),
                "{held} on {ground}"
            );
        }
    });
}

/// A dandelion that stands while a cobblestone is east of it, as its level shows it.
struct BesideCobblestone;
#[mod_api::sunrise_part]
impl BlockPart for BesideCobblestone {
    fn can_survive(&self, level: LevelSide<'_>, event: &mut CanSurviveEvent) -> Option<bool> {
        let LevelSide::Client(level) = level else {
            panic!("asked on a server");
        };
        let east = [event.pos.x + 1, event.pos.y, event.pos.z];
        Some(level.block_state(east) == level.block_state_id("minecraft:cobblestone"))
    }
}

struct NeighbourMod;
#[mod_api::__subscriptions]
impl Hooks for NeighbourMod {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        let dandelion = r.block_key("minecraft:dandelion");
        r.block_part(dandelion, BesideCobblestone);
    }
}

/// A placement's shape update asks a neighbour's survival part over the prediction's own
/// writes: the dandelion on stone, which its template pops off, stands because its part sees
/// the cobblestone just predicted beside it.
#[test]
#[ignore = "daily"]
fn a_neighbour_s_survival_part_reads_the_prediction_s_blocks() {
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "neighbour_test",
        Box::new(NeighbourMod),
    )]
    .into();
    on_a_client(mods, |game| {
        let state =
            |game: &Game<'_>, name: &str| game.pack.assets.registry.default_state(name).unwrap();
        for x in [0, 1] {
            game.world
                .set(x, 64, 0, state(game, "minecraft:stone"))
                .unwrap();
        }
        game.world
            .set(0, 65, 0, state(game, "minecraft:dandelion"))
            .unwrap();
        let (_, changes) = game
            .remote_can_place(&top_of(1, 0), &ItemStack::new("minecraft:cobblestone", 1))
            .expect("the cobblestone is placed");
        let at = |x| {
            changes
                .iter()
                .find(|(pos, _)| *pos == sunrise_core::position::BlockPos::new(x, 65, 0))
                .map(|&(_, state)| state)
        };
        assert_eq!(at(1), Some(state(game, "minecraft:cobblestone")));
        assert_eq!(at(0), None, "the dandelion stands");
    });
}

static GUIDE_ASKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct Counted;
#[mod_api::sunrise_part]
impl BlockPart for Counted {
    fn state_for_placement(
        &self,
        _: LevelSide<'_>,
        _: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        GUIDE_ASKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        None
    }
}

struct GuideMod;
#[mod_api::__subscriptions]
impl Hooks for GuideMod {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        let dirt = r.block_key("minecraft:dirt");
        r.block_part(dirt, Counted);
    }
}

/// The controller guide's look at what a click would place asks no part; the click does.
#[test]
#[ignore = "daily"]
fn the_guide_s_look_asks_no_placement_part_and_the_click_does() {
    use std::sync::atomic::Ordering;
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new("guide_test", Box::new(GuideMod))].into();
    on_a_client(mods, |game| {
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        game.world.set(0, 64, 0, stone).unwrap();
        let dirt = ItemStack::new("minecraft:dirt", 1);
        GUIDE_ASKS.store(0, Ordering::Relaxed);
        assert!(game.remote_would_place(&top_of(0, 0), &dirt));
        assert_eq!(GUIDE_ASKS.load(Ordering::Relaxed), 0);
        assert!(game.remote_can_place(&top_of(0, 0), &dirt).is_some());
        assert!(GUIDE_ASKS.load(Ordering::Relaxed) > 0);
    });
}

/// The comparator east of the clicked block's spot and what its part read of its signal.
const COMPARATOR: [i32; 3] = [2, 65, 0];
static COMPARATOR_READS: std::sync::Mutex<Vec<Option<i32>>> = std::sync::Mutex::new(Vec::new());

struct ReadsComparator;
#[mod_api::sunrise_part]
impl BlockPart for ReadsComparator {
    fn can_survive(&self, level: LevelSide<'_>, _: &mut CanSurviveEvent) -> Option<bool> {
        let LevelSide::Client(level) = level else {
            panic!("asked on a server");
        };
        let read = level.signal(COMPARATOR, mod_api::Direction::East);
        COMPARATOR_READS.lock().unwrap().push(read);
        None
    }
}

struct ComparatorMod;
#[mod_api::__subscriptions]
impl Hooks for ComparatorMod {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        let cobblestone = r.block_key("minecraft:cobblestone");
        r.block_part(cobblestone, ReadsComparator);
    }
}

/// A placement part reads a powered comparator's output from the client's own ticks, not from
/// the prediction's scratch ones: unknown while the client holds none, its value once it does.
#[test]
#[ignore = "daily"]
fn a_placement_part_reads_comparator_outputs_the_client_holds() {
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "comparator_test",
        Box::new(ComparatorMod),
    )]
    .into();
    on_a_client(mods, |game| {
        let registry = game.pack.assets.registry;
        let comparator = registry
            .resolve(&sunrise_world::chunk::BlockState {
                name: "minecraft:comparator".into(),
                properties: [("facing", "east"), ("powered", "true")]
                    .into_iter()
                    .map(|(key, value)| (key.into(), value.into()))
                    .collect(),
            })
            .unwrap();
        let [x, y, z] = COMPARATOR;
        game.world.set(x, y, z, comparator).unwrap();
        game.world
            .set(0, 64, 0, registry.default_state("minecraft:stone").unwrap())
            .unwrap();
        let cobblestone = ItemStack::new("minecraft:cobblestone", 1);
        let read = |game: &mut Game<'_>| {
            COMPARATOR_READS.lock().unwrap().clear();
            assert!(game.remote_can_place(&top_of(0, 0), &cobblestone).is_some());
            std::mem::take(&mut *COMPARATOR_READS.lock().unwrap())
        };
        let reads = read(game);
        assert!(
            !reads.is_empty() && reads.iter().all(Option::is_none),
            "{reads:?}"
        );
        game.block_ticks
            .comparator_outputs
            .insert(sunrise_core::position::BlockPos::new(x, y, z), 9);
        let reads = read(game);
        assert!(
            !reads.is_empty() && reads.iter().all(|r| *r == Some(9)),
            "{reads:?}"
        );
    });
}

/// The guess asks the live level's placement and survival questions over its own writes, with
/// the live ticks it reads; a look that asks no mods asks none.
#[test]
fn a_prediction_asks_the_live_level_over_its_writes() {
    use sunrise_core::{position::BlockPos, random::LegacyRandom};
    use sunrise_world::level::{LevelAccess, PreviewBlocks, Ticks};

    struct Live;
    impl sunrise_world::level::BlockEntityNeighbors for Live {}
    impl LevelAccess for Live {
        fn block_state(&self, _: BlockPos) -> BlockStateId {
            BlockStateId(0)
        }
        fn set_raw(&mut self, _: BlockPos, _: BlockStateId) -> Option<BlockStateId> {
            panic!("the guess must not write to the live level");
        }
        fn is_loaded(&self, _: BlockPos) -> bool {
            true
        }
        fn is_client_side(&self) -> bool {
            true
        }
        fn random(&mut self) -> &mut LegacyRandom {
            panic!("the guess must not draw from the live level");
        }
        fn game_time(&self) -> i64 {
            0
        }
        /// Survives over the block the preview set below it, from the live ticks.
        fn before_can_survive(
            &self,
            ticks: &Ticks,
            preview: Option<&PreviewBlocks>,
            pos: BlockPos,
            _: BlockStateId,
            survives: &mut Option<bool>,
        ) {
            assert_eq!(ticks.random_tick_value, 7, "asked with the live ticks");
            let below = BlockPos::new(pos.x, pos.y - 1, pos.z);
            *survives = Some(preview.is_some_and(|p| p.contains_key(&below)));
        }
    }

    let live = Live;
    let live_ticks = Ticks::new(7);
    let mut guess = DryLevel::new(&live);
    guess.reads = Some(&live_ticks);
    let pos = BlockPos::new(0, 1, 0);
    let ask = |guess: &DryLevel<'_>| {
        let mut survives = None;
        guess.before_can_survive(&Ticks::default(), None, pos, BlockStateId(1), &mut survives);
        survives
    };
    assert_eq!(ask(&guess), Some(false));
    guess.set_raw(BlockPos::new(0, 0, 0), BlockStateId(1));
    assert_eq!(ask(&guess), Some(true));
    guess.asks_mods = false;
    assert_eq!(ask(&guess), None);
}
