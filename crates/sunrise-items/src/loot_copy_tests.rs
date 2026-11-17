use super::*;
use serde_json::json;
use sunrise_core::{
    nbt::{Compound, Tag},
    random::LegacyRandom,
};
use sunrise_world::loot::{self, copy::BlockSource};

#[test]
fn copy_components_uses_available_sources_and_table_filters_in_both_evaluators() {
    let root = std::env::temp_dir().join(format!(
        "sunrise-copy-components-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let folder = root.join("data/test/loot_table");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        root.join("items.json"),
        json!({
            "minecraft:stick":{"minecraft:max_stack_size":64},
            "test:tool":{"minecraft:max_stack_size":1,"minecraft:damage":0,
                "minecraft:unbreakable":{},"minecraft:enchantment_glint_override":true}
        })
        .to_string(),
    )
    .unwrap();
    let base = Tag::compound([("value", Tag::String("before".into()))]);
    let block = BlockSource {
        nbt: Tag::compound([("id", Tag::String("test:source".into()))]),
        components: [
            ("test:keep".into(), Tag::ByteArray(vec![7, 252])),
            ("test:exclude".into(), Tag::Long(99)),
            ("test:ignored".into(), Tag::Int(8)),
        ]
        .into_iter()
        .collect(),
    };
    let potion = Tag::compound([("potion", Tag::String("minecraft:healing".into()))]);
    let tool_patch = Tag::compound([
        ("minecraft:damage", Tag::Int(8)),
        ("minecraft:potion_contents", potion.clone()),
        ("!minecraft:unbreakable", Tag::Compound(Compound::new())),
        ("test:keep", Tag::Long(17)),
    ]);
    let (damage, extra) = crate::item_data::decode_components(Some(&tool_patch));
    let held = ItemStack {
        item: Arc::from("test:tool"),
        count: 1,
        damage,
        extra,
    };
    let empty = loot::LootStack {
        item: String::new(),
        count: 1,
        damage: 0,
        enchantments: Vec::new(),
        potion: None,
        map: false,
        exploration: None,
        ominous_amplifier: None,
        stew_effects: Vec::new(),
        names: Vec::new(),
        components: Vec::new(),
    };
    let mut tool = loot::LootStack {
        item: "test:tool".into(),
        count: 1,
        damage: 8,
        ..empty.clone()
    };
    let Tag::Compound(patch) = &tool_patch else {
        unreachable!()
    };
    for (id, value) in patch {
        let id = id.to_string_lossy();
        tool.components
            .push(if let Some(base) = id.strip_prefix('!') {
                (base.to_owned(), None)
            } else {
                (id, Some(value.clone()))
            });
    }
    let mut entity = loot::LootEntity::new("test:entity");
    entity
        .components
        .insert("test:entity".into(), Tag::LongArray(vec![3, 4]));
    let cases = [
        (
            "intersection",
            json!({"source":"block_entity","include":["test:keep","test:exclude"],"exclude":["test:exclude"]}),
            true,
            false,
            false,
            vec![("test:keep", Tag::ByteArray(vec![7, 252]))],
        ),
        (
            "empty_include",
            json!({"source":"block_entity","include":[]}),
            true,
            false,
            false,
            vec![],
        ),
        (
            "without_include",
            json!({"source":"block_entity","exclude":["test:exclude","test:ignored"]}),
            true,
            false,
            false,
            vec![("test:keep", Tag::ByteArray(vec![7, 252]))],
        ),
        (
            "missing_block",
            json!({"source":"block_entity"}),
            false,
            false,
            false,
            vec![],
        ),
        (
            "tool",
            json!({"source":"tool","include":["minecraft:damage","minecraft:potion_contents","minecraft:enchantment_glint_override","minecraft:unbreakable","test:keep"],"exclude":["test:keep"]}),
            false,
            true,
            false,
            vec![
                ("minecraft:damage", Tag::Int(8)),
                ("minecraft:potion_contents", potion),
                ("minecraft:enchantment_glint_override", Tag::Byte(1)),
            ],
        ),
        (
            "missing_tool",
            json!({"source":"tool"}),
            false,
            false,
            false,
            vec![],
        ),
        (
            "entity",
            json!({"source":"this","include":["test:entity"]}),
            false,
            false,
            true,
            vec![("test:entity", Tag::LongArray(vec![3, 4]))],
        ),
        (
            "missing_entity",
            json!({"source":"this"}),
            false,
            false,
            false,
            vec![],
        ),
        (
            "missing_attacker",
            json!({"source":"attacker"}),
            false,
            false,
            true,
            vec![],
        ),
    ];
    for (name, function, _, _, _, _) in &cases {
        let mut function = function.clone();
        function["type"] = json!("minecraft:copy_components");
        let table = json!({"type":"minecraft:empty","pools":[{"rolls":1,"entries":[{
            "type":"minecraft:item","name":"minecraft:stick",
            "modifier":{"type":"minecraft:sequence","functions":[
                {"type":"minecraft:set_components","components":{"test:untouched":{"value":"before"}}}, function
            ]}
        }]}]});
        std::fs::write(folder.join(format!("{name}.json")), table.to_string()).unwrap();
    }
    let defaults = sunrise_world::item_defaults::read(&root).unwrap();
    let data = loot::LootData::load_with_types(
        &root,
        &loot::PackOverlay::NONE,
        &defaults,
        loot::types::Types::default(),
    )
    .unwrap();
    let mut items = Items::for_tests(HashMap::new());
    items.set_loot_data(&data);
    items.component_defaults = data.component_defaults().clone();
    for (name, _, has_block, has_tool, has_entity, copied) in cases {
        let mut expected: Compound = [("test:untouched".into(), base.clone())]
            .into_iter()
            .collect();
        expected.extend(copied.into_iter().map(|(id, tag)| (id.into(), tag)));
        let id = format!("test:{name}");
        for item_engine in [false, true] {
            // A queried stack is deliberately not the TOOL.
            let queried = loot::LootStack {
                item: "test:not_the_tool".into(),
                count: 1,
                components: vec![("test:wrong".into(), Some(Tag::Int(42)))],
                ..empty.clone()
            };
            let query = std::rc::Rc::new(loot::query::LootQuery::new(
                Some(queried),
                has_tool.then(|| tool.clone()),
            ));
            let actual = loot::query::with_query(query, || {
                loot::copy::with_block_source(has_block.then_some(&block), || {
                    let mut random = LegacyRandom::new(1234);
                    if item_engine {
                        let predicate = |_: &Value, _: &loot::LootEntity| true;
                        let mut output = items
                            .roll_loot_table(
                                &id,
                                has_tool.then_some(&held),
                                has_entity.then_some((&entity, &predicate)),
                                &mut random,
                            )
                            .unwrap();
                        assert_eq!(output.len(), 1);
                        crate::item_data::encode_components(&output.remove(0)).unwrap()
                    } else {
                        let mut output = if has_entity {
                            data.roll_entity(
                                &id,
                                &mut random,
                                &loot::EntityLootParams {
                                    this: entity.clone(),
                                    ..Default::default()
                                },
                            )
                        } else {
                            data.roll(&id, &mut random, 0.0, None, None)
                        };
                        assert_eq!(output.len(), 1);
                        let mut patch = Compound::new();
                        for (id, value) in output.remove(0).components {
                            if let Some(value) = value {
                                patch.insert(id.as_str().into(), value);
                            } else {
                                patch.remove(&id.as_str().into());
                            }
                        }
                        Tag::Compound(patch)
                    }
                })
            });
            assert_eq!(
                actual,
                Tag::Compound(expected.clone()),
                "{name}, items = {item_engine}"
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
