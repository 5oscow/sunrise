//! `ItemsClientHost::creative_tabs` lists the item tabs with their stacks and the build's
//! permissions; `item_uses_block_light` tells a block item's model from a flat one's.
#![cfg(test)]
use super::creative_tabs_read::item_tabs;
use sunrise_client::creative::{Creative, Names};

fn creative() -> Creative {
    let stack = |id: &str| format!("{{count:1,id:\"minecraft:{id}\"}}");
    let named =
        "{count:1,id:\"minecraft:stone\",components:{\"minecraft:custom_name\":\"Plinth\"}}";
    Creative::from_json(&serde_json::json!({"schema": 3, "tabs": [
        {"id": "minecraft:building_blocks", "type": "CATEGORY", "row": "TOP", "column": 0, "title": "Building Blocks",
         "icon": stack("bricks"), "visible": true, "items": [stack("stone"), named]},
        {"id": "minecraft:op_blocks", "type": "CATEGORY", "row": "BOTTOM", "column": 5, "title": "Operator Utilities",
         "icon": stack("command_block"), "visible": false,
         "items": [stack("command_block")], "permissions_only": [0]},
        {"id": "minecraft:search", "type": "SEARCH", "row": "TOP", "column": 6, "title": "Search Items",
         "icon": stack("compass"), "visible": true,
         "items": [stack("stone"), stack("command_block")],
         "lines": [["Stone"], ["Command Block"]], "tags": [[], []], "permissions_only": [1]},
        {"id": "minecraft:inventory", "type": "INVENTORY", "row": "BOTTOM", "column": 6, "title": "Survival Inventory",
         "icon": stack("chest"), "visible": true, "items": []}
    ]}))
    .unwrap()
}

/// Only item tabs, in order, with components kept, mod stacks after the tab's own, and the
/// operator tab present only when the build had permissions.
#[test]
fn item_tabs_with_their_stacks_and_the_builds_permissions() {
    let mut c = creative();
    let mod_stack = sunrise_client::items::ItemStack::new("minecraft:diamond", 1);
    let added = vec![("minecraft:building_blocks".to_owned(), vec![mod_stack])];
    c.build(false, added.clone(), &Names::default());
    assert!(!c.permissions());
    let tabs: Vec<_> = item_tabs(&c).collect();
    let ids: Vec<&str> = tabs.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["minecraft:building_blocks"]);
    let shown = &tabs[0].items;
    let items: Vec<&str> = shown.iter().map(|s| &*s.item).collect();
    assert_eq!(
        items,
        ["minecraft:stone", "minecraft:stone", "minecraft:diamond"]
    );
    assert!(shown[0].extra.is_none());
    assert!(shown[1].extra.is_some());

    c.build(true, added, &Names::default());
    assert!(c.permissions());
    let ids: Vec<&str> = item_tabs(&c).map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["minecraft:building_blocks", "minecraft:op_blocks"]);
}

/// A block item's model is lit from the side, a flat item's from the front.
#[test]
#[ignore = "daily"]
fn block_items_use_block_light_and_flat_items_do_not() {
    crate::game::native_entity_refs::tests::with_client_game(|game| {
        let light = |id: &str, context| {
            let stack = sunrise_client::items::ItemStack::new(id, 1);
            game.item_uses_block_light(&stack, context)
        };
        for context in [
            mod_api::ItemDisplayContext::Gui,
            mod_api::ItemDisplayContext::Ground,
        ] {
            assert!(light("minecraft:stone", context));
            assert!(light("minecraft:chest", context));
            assert!(!light("minecraft:stick", context));
            assert!(!light("minecraft:diamond", context));
        }
        assert!(!light("minecraft:air", mod_api::ItemDisplayContext::Gui));
    });
}
