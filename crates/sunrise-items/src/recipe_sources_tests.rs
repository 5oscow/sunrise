//! #1381: a recipe's list elements carry no load conditions of their own.
use super::*;
use serde_json::json;

#[test]
fn a_recipe_keeps_an_ingredients_conditions_as_data() {
    let root = std::env::temp_dir().join(format!("sunrise-1381-recipe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("data/minecraft/recipe");
    std::fs::create_dir_all(&dir).unwrap();
    let recipe = json!({
        "type": "minecraft:crafting_shapeless",
        "ingredients": [
            "minecraft:stick",
            {"item": "ic2:tin", "neoforge:conditions": [{"type": "absent:condition"}]}
        ],
        "result": {"id": "minecraft:stone"}
    });
    std::fs::write(dir.join("tin.json"), recipe.to_string()).unwrap();
    let conditions = sunrise_world::conditions::LoadConditions::new([])
        .with_lookup(std::collections::BTreeMap::new(), |_, _| None);
    let overlay = PackOverlay {
        files: &[],
        conditions: &conditions,
    };
    let loaded = sources(&root, &overlay).expect("no diagnostic refuses the load");
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(loaded, vec![("tin".to_owned(), recipe)]);
}

#[test]
fn filters_and_readers_see_the_decoded_type() {
    let root = std::env::temp_dir().join(format!("sunrise-2077-filter-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let recipes = root.join("data/minecraft/recipe");
    std::fs::create_dir_all(&recipes).unwrap();
    std::fs::create_dir_all(root.join("data/example/recipe_filter")).unwrap();
    std::fs::write(
        root.join(crate::content_namespaces::MARKER),
        r#"["example"]"#,
    )
    .unwrap();
    std::fs::write(
        root.join("data/example/recipe_filter/f.json"),
        r#"{"filters":[{"type":"minecraft:smelting","remove":true}]}"#,
    )
    .unwrap();
    std::fs::write(recipes.join("bare_smelting.json"), r#"{"type":"smelting"}"#).unwrap();
    std::fs::write(
        recipes.join("colon_smelting.json"),
        r#"{"type":":smelting"}"#,
    )
    .unwrap();
    std::fs::write(
        recipes.join("whole_smelting.json"),
        r#"{"type":"minecraft:smelting"}"#,
    )
    .unwrap();
    std::fs::write(
        recipes.join("bare_shaped.json"),
        r#"{"type":"crafting_shaped"}"#,
    )
    .unwrap();
    // A false condition silences the file before its type is decoded.
    std::fs::write(
        recipes.join("skipped.json"),
        r#"{"type":"Not An Id","neoforge:conditions":[{"type":"neoforge:not","value":{"type":"neoforge:always"}}]}"#,
    )
    .unwrap();
    let conditions = sunrise_world::conditions::LoadConditions::new([])
        .with_lookup(std::collections::BTreeMap::new(), |_, _| None);
    let overlay = PackOverlay {
        files: &[],
        conditions: &conditions,
    };
    let loaded = sources(&root, &overlay).expect("the load is not refused");
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(
        loaded,
        vec![(
            "bare_shaped".to_owned(),
            json!({"type": "minecraft:crafting_shaped"})
        )]
    );
}

#[test]
fn a_type_that_does_not_decode_is_refused_by_category() {
    let decode = |mut recipe: Value| decode_type(&mut recipe).map(|()| recipe);
    assert_eq!(decode(json!({"type": "a:b"})), Ok(json!({"type": "a:b"})));
    assert_eq!(
        decode(json!({"type": "b"})),
        Ok(json!({"type": "minecraft:b"}))
    );
    assert_eq!(
        decode(json!({"type": ":b"})),
        Ok(json!({"type": "minecraft:b"}))
    );
    assert_eq!(decode(json!([])), Err(TypeRefusal::NotAnObject));
    assert_eq!(decode(json!({"type": 1})), Err(TypeRefusal::MissingType));
    assert_eq!(decode(json!({})), Err(TypeRefusal::MissingType));
    assert!(matches!(
        decode(json!({"type": "Crafting_shapeless"})),
        Err(TypeRefusal::MalformedId(_))
    ));
}
