//! The creative tabs a client mod reads (`CreativeModeTabs.tabs()`), each with the stacks it shows
//! (`CreativeModeTab.getDisplayItems`), for mods that filter or list items by tab.
use crate::ItemStack;

/// One item tab of the creative inventory, as [`ItemsClientHost::creative_tabs`](crate::ItemsClientHost::creative_tabs)
/// lists it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
#[derive(sunrise_mod_api_macros::Wire)]
pub struct CreativeModeTabInfo {
    /// The tab's registry id (`minecraft:ingredients`, or `<mod id>:<name>` for a mod's tab).
    pub id: String,
    /// `getDisplayItems`: the stacks the tab shows, in order, with their components. Includes
    /// what mods added in `build_creative_mode_tab_contents`, which follow the tab's own, and
    /// leaves out the stacks only a build with permissions lists when the last build had none.
    pub display_items: Vec<ItemStack>,
}

impl CreativeModeTabInfo {
    /// A tab with the registry id `id` showing `display_items`.
    pub fn new(id: String, display_items: Vec<ItemStack>) -> Self {
        Self { id, display_items }
    }
}

/// The creative tabs and the context they were last built in
/// (`CreativeModeTab.ItemDisplayParameters`), as [`ItemsClientHost::creative_tabs`](crate::ItemsClientHost::creative_tabs)
/// reads them.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
#[derive(sunrise_mod_api_macros::Wire)]
pub struct CreativeModeTabs {
    /// The build's `hasPermissions`: whether the operator items tab is allowed (the player can
    /// use game master blocks and the Operator Items Tab option is on).
    pub has_permissions: bool,
    /// The item tabs in `CreativeModeTabs.tabs()` order: not the search, saved toolbars or
    /// survival inventory tabs.
    pub tabs: Vec<CreativeModeTabInfo>,
}

impl CreativeModeTabs {
    /// The tabs built with `has_permissions`.
    pub fn new(has_permissions: bool, tabs: Vec<CreativeModeTabInfo>) -> Self {
        Self {
            has_permissions,
            tabs,
        }
    }
    /// The tab with registry id `id`.
    pub fn tab(&self, id: &str) -> Option<&CreativeModeTabInfo> {
        self.tabs.iter().find(|tab| tab.id == id)
    }
    /// `CreativeModeTab.contains`-style membership: the tabs that show `stack`, by item and
    /// components, in tab order.
    pub fn tabs_showing<'a>(
        &'a self,
        stack: &'a ItemStack,
    ) -> impl Iterator<Item = &'a CreativeModeTabInfo> {
        self.tabs.iter().filter(move |tab| {
            tab.display_items
                .iter()
                .any(|shown| shown.is_stackable_with(stack))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ItemKey;

    /// A tab is found by id, and a stack is shown by the tabs that list its item with the same
    /// components: the same item with other components is not.
    #[test]
    fn tabs_are_found_by_id_and_membership_follows_item_and_components() {
        let plain = ItemStack::of(ItemKey(1));
        let mut damaged = ItemStack::of(ItemKey(1));
        damaged.set_damage(3);
        let other = ItemStack::of(ItemKey(2));
        let tabs = CreativeModeTabs::new(
            true,
            vec![
                CreativeModeTabInfo::new("minecraft:building_blocks".into(), vec![plain.clone()]),
                CreativeModeTabInfo::new("a:tools".into(), vec![damaged.clone(), other.clone()]),
                CreativeModeTabInfo::new("a:both".into(), vec![other.clone(), plain.clone()]),
            ],
        );
        assert_eq!(tabs.tab("a:tools").map(|t| t.display_items.len()), Some(2));
        assert!(tabs.tab("a:missing").is_none());
        let ids = |stack: &ItemStack| -> Vec<String> {
            tabs.tabs_showing(stack).map(|t| t.id.clone()).collect()
        };
        assert_eq!(ids(&plain), ["minecraft:building_blocks", "a:both"]);
        assert_eq!(ids(&damaged), ["a:tools"]);
        assert_eq!(ids(&other), ["a:tools", "a:both"]);
    }
}
