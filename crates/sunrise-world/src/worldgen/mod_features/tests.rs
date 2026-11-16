//! Mod feature types through the engine: decoding, the caller's random, the placement chain's
//! order, nesting, and what a feature's answers do to the world.
use super::*;
use crate::worldgen::anchor::WorldGenerationContext;
use crate::worldgen::feature::{FeatureBlocks, FeatureWorld};
use crate::worldgen::feature_graph::{
    FeatureDefinition, FeatureExecutionError, PlacedFeatureDefinition,
};
use crate::worldgen::placement::{PlacementContext, PlacementModifier, PlacementQueryError};
use crate::worldgen::value_provider::IntProvider;
use std::sync::Mutex;
use sunrise_core::random::{LegacyRandom, WorldgenRandom};

/// A 16-block-tall column world: reads and writes inside y 0..16, refusals outside, and
/// every write kept in order.
#[derive(Default)]
struct World {
    writes: Vec<([i32; 3], BlockStateId, i32)>,
}
impl FeatureWorld for World {
    type Error = PlacementQueryError;
    fn block(&mut self, p: [i32; 3]) -> Result<BlockStateId, Self::Error> {
        if !(0..16).contains(&p[1]) {
            return Err(PlacementQueryError::Unavailable);
        }
        Ok(self
            .writes
            .iter()
            .rev()
            .find(|w| w.0 == p)
            .map_or(BlockStateId(0), |w| w.1))
    }
    fn set_block(&mut self, p: [i32; 3], s: BlockStateId, flags: i32) -> Result<bool, Self::Error> {
        if !(0..16).contains(&p[1]) {
            return Err(PlacementQueryError::Unavailable);
        }
        self.writes.push((p, s, flags));
        Ok(true)
    }
}
impl PlacementContext for World {
    fn generation(&self) -> WorldGenerationContext {
        WorldGenerationContext::new(0, 16, 8)
    }
    fn min_y(&mut self) -> Result<i32, PlacementQueryError> {
        Ok(0)
    }
    fn max_y(&mut self) -> Result<i32, PlacementQueryError> {
        Ok(15)
    }
    fn height(&mut self, _: HeightmapType, _: i32, _: i32) -> Result<i32, PlacementQueryError> {
        Ok(4)
    }
    fn block_matches_tag(&mut self, p: [i32; 3], tag: &str) -> Result<bool, PlacementQueryError> {
        Ok(tag == "minecraft:dirt" && p[1] == 0)
    }
}
fn blocks() -> FeatureBlocks {
    FeatureBlocks {
        air: BlockStateId(0),
        stone: BlockStateId(1),
        cobblestone: BlockStateId(2),
        end_stone: BlockStateId(3),
        obsidian: BlockStateId(4),
    }
}

/// What a test type does when placed: `Marks` draws `nextInt(1)` and writes stone where it
/// is placed, recording the origin and the draw; `Column` writes its config's height of
/// stone, then answers false; `Fails` writes once, then fails.
#[derive(Clone, Copy)]
enum Kind {
    Marks,
    Column,
    Fails,
}
type Log = Arc<Mutex<Vec<([i32; 3], i32)>>>;
struct Test(Kind, Log);
impl ModFeatureType for Test {
    fn prepare_config(&self, config: &serde_json::Value) -> Result<Arc<Tag>, String> {
        match config.get("height") {
            None => Ok(Arc::new(Tag::Int(1))),
            Some(serde_json::Value::Number(n)) if n.as_i64().is_some_and(|n| n > 0) => {
                Ok(Arc::new(Tag::Int(n.as_i64().unwrap_or(1) as i32)))
            }
            Some(_) => Err("height must be a positive number".into()),
        }
    }
    fn place(
        &self,
        target: &mut dyn ModFeatureTarget,
        random: &mut dyn ModFeatureRandom,
        origin: [i32; 3],
        config: &Arc<Tag>,
    ) -> Result<bool, String> {
        match self.0 {
            Kind::Marks => {
                let draw = random.int(1);
                self.1.lock().unwrap().push((origin, draw));
                Ok(target.set_block(origin, BlockStateId(1), 3))
            }
            Kind::Column => {
                let Tag::Int(height) = **config else {
                    return Err("config".into());
                };
                for dy in 0..height {
                    target.set_block([origin[0], origin[1] + dy, origin[2]], BlockStateId(1), 2);
                }
                Ok(false)
            }
            Kind::Fails => {
                target.set_block(origin, BlockStateId(2), 3);
                Err("no room".into())
            }
        }
    }
}
fn types(log: &Log) -> ModFeatureTypes {
    [
        ("test:marks", Kind::Marks),
        ("test:column", Kind::Column),
        ("test:fails", Kind::Fails),
    ]
    .into_iter()
    .map(|(id, kind)| {
        let ty: Arc<dyn ModFeatureType> = Arc::new(Test(kind, log.clone()));
        (id.to_owned(), ty)
    })
    .collect()
}

/// A registered type's id decodes to the mod's feature, with its config converted once;
/// without the type (or with no mod types at all) the id is the unknown type it was before.
#[test]
fn a_registered_type_decodes_and_an_unregistered_one_still_refuses() {
    let log = Log::default();
    with_test_feature_types(&types(&log), || {
        let feature = FeatureDefinition::from_json(r#"{"type":"test:column","height":3}"#).unwrap();
        let FeatureDefinition::Mod(feature) = feature else {
            panic!("{feature:?}")
        };
        assert_eq!(feature.id, "test:column");
        assert_eq!(*feature.config, Tag::Int(3));
        assert!(FeatureDefinition::from_json(r#"{"type":"test:missing"}"#).is_err());
        // A config the type refuses refuses the definition, naming the type.
        let error = FeatureDefinition::from_json(r#"{"type":"test:column","height":"x"}"#)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("test:column") && error.contains("positive"),
            "{error}"
        );
    });
    assert!(FeatureDefinition::from_json(r#"{"type":"test:column"}"#).is_err());
}

/// The feature draws from the caller's own random: its `nextInt(1)` is 0 and spends one
/// draw, so the caller's next draws are the jar's (`oracle_tests`: 13, 3, then
/// -7261648964369397258 from seed 0).
#[test]
fn the_feature_draws_from_the_callers_stream() {
    let log = Log::default();
    with_test_feature_types(&types(&log), || {
        let feature = FeatureDefinition::from_json(r#"{"type":"test:marks"}"#).unwrap();
        let mut random = WorldgenRandom::new(LegacyRandom::new(0));
        let mut world = World::default();
        assert!(
            feature
                .place(&mut world, &mut random, [0, 4, 0], blocks())
                .unwrap()
        );
        assert_eq!(*log.lock().unwrap(), [([0, 4, 0], 0)]);
        assert_eq!(world.writes, [([0, 4, 0], BlockStateId(1), 3)]);
        assert_eq!(random.next_int_bound(16).unwrap(), 13);
        assert_eq!(random.next_int_bound(16).unwrap(), 3);
        assert_eq!(random.next_long(), -7261648964369397258);
    });
}

/// Through a placed feature's modifiers, depth first as `FeaturePlacer.place` runs them: a
/// duplicate origin is two calls, and the feature runs at the first position before the
/// next modifier draws for the second. The expected positions are drawn from a copy of the
/// same stream in that order.
#[test]
fn modifiers_feed_the_feature_in_vanilla_order_without_deduplication() {
    let log = Log::default();
    with_test_feature_types(&types(&log), || {
        let placed = PlacedFeatureDefinition {
            feature: Box::new(FeatureDefinition::from_json(r#"{"type":"test:marks"}"#).unwrap()),
            placement: vec![
                PlacementModifier::Count(IntProvider::Constant(2)),
                PlacementModifier::InSquare,
            ],
        };
        let mut random = WorldgenRandom::new(LegacyRandom::new(42));
        let mut expected = random.clone();
        let mut world = World::default();
        assert!(
            placed
                .place(&mut world, &mut random, [16, 4, 32], blocks())
                .unwrap()
        );
        let mut want = Vec::new();
        for _ in 0..2 {
            let x = expected.next_int_bound(16).unwrap();
            let z = expected.next_int_bound(16).unwrap();
            let draw = expected.next_int_bound(1).unwrap();
            want.push(([16 + x, 4, 32 + z], draw));
        }
        assert_eq!(*log.lock().unwrap(), want);
        assert_eq!(random.next_long(), expected.next_long());
    });
}

/// A nested feature (a random selector's default) reaches the mod's type too.
#[test]
fn a_nested_feature_reaches_the_mods_type() {
    let log = Log::default();
    with_test_feature_types(&types(&log), || {
        let feature = FeatureDefinition::from_json(
            r#"{"type":"minecraft:random_selector","features":[],
                "default":{"feature":{"type":"test:column","height":2},"placement":[]}}"#,
        )
        .unwrap();
        let mut world = World::default();
        let mut random = LegacyRandom::new(1);
        // The column answers false; the selector passes its answer on.
        assert!(
            !feature
                .place(&mut world, &mut random, [3, 1, 3], blocks())
                .unwrap()
        );
        assert_eq!(
            world.writes,
            [
                ([3, 1, 3], BlockStateId(1), 2),
                ([3, 2, 3], BlockStateId(1), 2)
            ]
        );
    });
}

/// `Ok(false)` keeps what the feature wrote, as vanilla does; a write the world refuses is
/// false to the mod and changes nothing; a failure stops the placement, naming the type.
#[test]
fn false_keeps_writes_refused_writes_are_false_and_failures_name_the_type() {
    let log = Log::default();
    with_test_feature_types(&types(&log), || {
        let column = FeatureDefinition::from_json(r#"{"type":"test:column","height":4}"#).unwrap();
        let mut world = World::default();
        let mut random = LegacyRandom::new(1);
        // From y 14: 14 and 15 are written, 16 and 17 are outside and refused.
        assert!(
            !column
                .place(&mut world, &mut random, [0, 14, 0], blocks())
                .unwrap()
        );
        assert_eq!(
            world.writes.iter().map(|w| w.0).collect::<Vec<_>>(),
            [[0, 14, 0], [0, 15, 0]]
        );
        let fails = FeatureDefinition::from_json(r#"{"type":"test:fails"}"#).unwrap();
        let mut world = World::default();
        match fails.place(&mut world, &mut random, [0, 1, 0], blocks()) {
            Err(FeatureExecutionError::Mod(reason)) => {
                assert_eq!(reason, "test:fails: no room")
            }
            other => panic!("{other:?}"),
        }
    });
}

/// The target answers reads, tags and heights from the caller's world, and `None` outside it.
#[test]
fn the_target_reads_the_callers_world() {
    let mut world = World::default();
    world.writes.push(([1, 0, 1], BlockStateId(7), 3));
    let mut target = Target(&mut world);
    assert_eq!(target.block([1, 0, 1]), Some(BlockStateId(7)));
    assert_eq!(target.block([1, -1, 1]), None);
    assert_eq!(target.block_in_tag([1, 0, 1], "minecraft:dirt"), Some(true));
    assert_eq!(
        target.block_in_tag([1, 1, 1], "minecraft:dirt"),
        Some(false)
    );
    assert_eq!((target.min_y(), target.max_y()), (Some(0), Some(15)));
    assert_eq!(target.height(HeightmapType::WorldSurfaceWg, 0, 0), Some(4));
    assert!(target.registry().is_none());
    // A bound below 1 answers 0 without a draw.
    let mut random = LegacyRandom::new(3);
    let mut copy = random.clone();
    assert_eq!(Draws(&mut random).int(0), 0);
    assert_eq!(random.next_long(), copy.next_long());
}
