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
