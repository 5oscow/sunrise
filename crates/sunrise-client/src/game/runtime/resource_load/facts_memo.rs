//! Immutable file-derived tables shared only by test resource loads.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use sunrise_items::blocks::GameplayProjection;
use sunrise_world::registry::{BlockRegistry, BlockStateId};

type Tables = (BlockFacts, Vec<GameplayProjection>);
#[derive(Hash, PartialEq, Eq)]
struct Key {
    root: PathBuf,
    inputs: [u8; 32],
    parallel: bool,
}
#[derive(Default)]
struct Memo(Mutex<HashMap<Key, Arc<Tables>>>);
static MEMO: OnceLock<Memo> = OnceLock::new();

pub(super) fn load(
    root: &std::path::Path,
    registry: &BlockRegistry,
    behaviours: &serde_json::Value,
    parallel: bool,
) -> std::result::Result<Tables, String> {
    // Fluid classification is the fact loader's one registration-dependent branch.
    if !sunrise_world::fluid::mod_fluids().is_empty() {
        return BlockFacts::load_with_projection(root, registry, behaviours, parallel)
            .map_err(|e| e.to_string());
    }
    MEMO.get_or_init(Memo::default)
        .load(root, registry, behaviours, parallel)
        .map(|tables| tables.as_ref().clone())
}

impl Memo {
    fn load(
        &self,
        root: &std::path::Path,
        registry: &BlockRegistry,
        behaviours: &serde_json::Value,
        parallel: bool,
    ) -> std::result::Result<Arc<Tables>, String> {
        let mut hash = Sha256::new();
        // The manifest identifies preparation; facts defines the usual registry.
        // The projection loader reads gameplay facts and the caller supplies behaviours.
        for name in [
            "sunrise-resources.json",
            "facts.json",
            "gameplay-facts.json",
            "block-behaviours.json",
        ] {
            let bytes = sunrise_world::json_file::read_bytes(&root.join(name))
                .map_err(|e| e.to_string())?;
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        // Include the actual document, not just the file from which the caller read it.
        let bytes = serde_json::to_vec(behaviours).map_err(|e| e.to_string())?;
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
        // Content mods may supply a different registry over the same prepared root.
        // Names alone are insufficient: waterlogging/level properties affect fluids.
        hash.update((registry.len() as u64).to_le_bytes());
        for id in 0..registry.len() {
            let state = registry
                .state(BlockStateId(id as u32))
                .ok_or("missing registry state")?;
            hash.update((state.properties.len() as u64).to_le_bytes());
            for text in std::iter::once(state.name.as_str()).chain(
                state
                    .properties
                    .iter()
                    .flat_map(|(k, v)| [k.as_str(), v.as_str()]),
            ) {
                hash.update((text.len() as u64).to_le_bytes());
                hash.update(text.as_bytes());
            }
        }
        hash.update(
            registry
                .default_state("minecraft:air")
                .map(|id| id.0)
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        let key = Key {
            root: root.canonicalize().map_err(|e| e.to_string())?,
            inputs: hash.finalize().into(),
            parallel,
        };
        let mut memo = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(tables) = memo.get(&key) {
            return Ok(tables.clone());
        }
        let tables = Arc::new(
            BlockFacts::load_with_projection(root, registry, behaviours, parallel)
                .map_err(|e| e.to_string())?,
        );
        // Bound retention across temporary content roots without evicting the common root.
        if memo.len() < 8 {
            memo.insert(key, tables.clone());
        }
        Ok(tables)
    }
}

#[test]
fn identical_inputs_hit_but_changed_behaviours_and_registries_reload() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let registry_json = br#"{"states":[["minecraft:air",true,false,"","minecraft:air"],["minecraft:stone",true,false,"","minecraft:stone"]],"defaults":{"minecraft:air":0,"minecraft:stone":1}}"#;
    std::fs::write(root.join("sunrise-resources.json"), b"{}").unwrap();
    std::fs::write(root.join("facts.json"), registry_json).unwrap();
    std::fs::write(
        root.join("gameplay-facts.json"),
        br#"{"states":[{"id":0},{"id":1,"hardness":2}]}"#,
    )
    .unwrap();
    let mut behaviours = serde_json::json!({"states":[[0,0],[0,0]],"blocks":[{"name":"minecraft:air","class":["Block"]},{"name":"minecraft:stone","class":["Block"]},{"name":"minecraft:dirt","class":["Block"]}]});
    std::fs::write(root.join("block-behaviours.json"), behaviours.to_string()).unwrap();
    let registry = BlockRegistry::from_facts_json(&registry_json[..]).unwrap();
    let memo = Memo::default();
    let first = memo.load(root, &registry, &behaviours, true).unwrap();
    let hit = memo.load(root, &registry, &behaviours, true).unwrap();
    assert!(Arc::ptr_eq(&first, &hit));
    let fresh = BlockFacts::load_with_projection(root, &registry, &behaviours, true).unwrap();
    assert_eq!(
        format!("{:?}", hit.0.states),
        format!("{:?}", fresh.0.states)
    );
    assert_eq!(hit.0.air, fresh.0.air);
    assert_eq!(hit.0.woods, fresh.0.woods);
    assert_eq!(hit.0.sign_texts, fresh.0.sign_texts);
    assert_eq!(hit.1, fresh.1);
    behaviours["states"][1] = serde_json::json!([63, 1]);
    std::fs::write(root.join("block-behaviours.json"), behaviours.to_string()).unwrap();
    let changed = memo.load(root, &registry, &behaviours, true).unwrap();
    assert!(!Arc::ptr_eq(&hit, &changed));
    assert_eq!(changed.0.states[1].sturdy_full, 63);
    assert!(changed.0.states[1].legacy_solid);
    let other_json = String::from_utf8(registry_json.to_vec())
        .unwrap()
        .replace("minecraft:stone", "minecraft:dirt");
    let other_registry = BlockRegistry::from_facts_json(other_json.as_bytes()).unwrap();
    let other = memo.load(root, &other_registry, &behaviours, true).unwrap();
    assert!(!Arc::ptr_eq(&changed, &other));
    assert_eq!(&*other.0.states[1].name, "minecraft:dirt");
}
