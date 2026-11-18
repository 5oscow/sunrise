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
