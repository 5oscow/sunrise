//! `ItemsClientHost::creative_tabs` lists the item tabs with their stacks and the build's
//! permissions; `item_uses_block_light` tells a block item's model from a flat one's.
#![cfg(test)]
use super::creative_tabs_read::item_tabs;
use sunrise_client::creative::{Creative, Names};

fn creative() -> Creative {
    let stack = |id: &str| format!("{{count:1,id:\"minecraft:{id}\"}}");
    let named =
        "{count:1,id:\"minecraft:stone\",components:{\"minecraft:custom_name\":\"Plinth\"}}";
    let row = |snbt: &str| serde_json::json!({"kind":"stack","visibility":"PARENT_AND_SEARCH_TABS","snbt":snbt});
    Creative::from_json(&serde_json::json!({"schema":4,"item_features":{},"potion_features":{},"tabs":[
        {"id":"minecraft:building_blocks","type":"CATEGORY","build":[row(&stack("stone")),row(named)]},
        {"id":"minecraft:op_blocks","type":"CATEGORY","build":[],"operator_build":[row(&stack("command_block"))]},
        {"id":"minecraft:search","type":"SEARCH","build":[]},
        {"id":"minecraft:inventory","type":"INVENTORY","build":[]}
    ]})).unwrap()
}

/// Only item tabs, in order, with components kept, mod stacks after the tab's own, and the
/// operator tab present only when the build had permissions.
#[test]
fn item_tabs_with_their_stacks_and_the_builds_permissions() {
    let mut c = creative();
    let mod_stack = sunrise_client::items::ItemStack::new("minecraft:diamond", 1);
    let added = vec![("minecraft:building_blocks".to_owned(), vec![mod_stack])];
    let items = sunrise_items::items::Items::empty();
    let holders = sunrise_items::creative_catalog::Holders::default();
    let params = sunrise_items::creative_catalog::Parameters {
        operator: false,
        features: &[],
        holders: &holders,
        items: &items,
    };
    c.build(params, added.clone(), &Names::default());
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

    c.build(
        sunrise_items::creative_catalog::Parameters {
            operator: true,
            ..params
        },
        added,
        &Names::default(),
    );
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
