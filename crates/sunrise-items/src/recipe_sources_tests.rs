//! #1381: a recipe's list elements carry no load conditions of their own.
use super::*;
use serde_json::json;

#[test]
fn unknown_conditions_report_each_winning_recipe_and_refuse_the_load() {
    let root = std::env::temp_dir().join(format!("sunrise-recipe-refusal-{}", std::process::id()));
    std::fs::create_dir_all(root.join("data/minecraft/recipe")).unwrap();
    let bad = |id: &str| {
        json!({"neoforge:conditions":[{"type":id}]})
            .to_string()
            .into_bytes()
    };
    let files = [
        (
            "recipe/demo:a".to_owned(),
            vec![b"{}".to_vec(), bad("minecraft:missing")],
        ),
        ("recipe/demo:b".to_owned(), vec![bad("unrelated:missing")]),
        (
            "recipe/demo:c".to_owned(),
            vec![bad("unrelated:loser"), b"{}".to_vec()],
        ),
    ];
    let conditions = sunrise_world::conditions::LoadConditions::new([]);
    let result = sources(
        &root,
        &PackOverlay {
            files: &files,
            conditions: &conditions,
        },
    );
    std::fs::remove_dir_all(root).unwrap();
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("data/demo/recipe/a.json: unknown load conditions [minecraft:missing]"),
        "{error}"
    );
    assert!(
        error.contains("data/demo/recipe/b.json: unknown load conditions [unrelated:missing]"),
        "{error}"
    );
    assert!(!error.contains("unrelated:loser"), "{error}");
}

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
    let conditions = sunrise_world::conditions::LoadConditions::new([]);
    let overlay = PackOverlay {
        files: &[],
        conditions: &conditions,
    };
    let loaded = sources(&root, &overlay).expect("no diagnostic refuses the load");
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(loaded, vec![("tin".to_owned(), recipe)]);
}
