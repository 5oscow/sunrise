//! `ItemsClientHost::creative_tabs` and `ClientRenderClientHost::item_uses_block_light`: the
//! creative tabs' contents and an item model's lighting, read by a client mod.
use super::*;
use crate::game::api_values::{TryIntoApi, engine_display_context};

impl Game<'_> {
    /// The creative inventory's item tabs with their stacks, and the `hasPermissions` they were
    /// last built with.
    pub(in crate::game) fn creative_tabs(&self) -> mod_api::CreativeModeTabs {
        creative_tabs_of(&self.creative_screen)
    }
    /// `ItemStackRenderState.usesBlockLight` of `stack` resolved for `context`.
    pub(in crate::game) fn item_uses_block_light(
        &self,
        stack: &ItemStack,
        context: mod_api::ItemDisplayContext,
    ) -> bool {
        let Some(display) = engine_display_context(context) else {
            return false;
        };
        self.resolve_item(stack, display, None, 0)
            .uses_block_light()
    }
}

/// The tabs `creative_tabs` lists: the item tabs, not the search, saved toolbars or survival
/// inventory tabs, in `CreativeModeTabs.tabs()` order.
pub(super) fn item_tabs(
    creative: &sunrise_client::creative::Creative,
) -> impl Iterator<Item = &sunrise_client::creative::Tab> {
    creative
        .tabs
        .iter()
        .filter(|tab| !tab.search && !tab.inventory && !tab.hotbar)
}

/// The item tabs of `creative` with their stacks, and its `hasPermissions`.
fn creative_tabs_of(creative: &sunrise_client::creative::Creative) -> mod_api::CreativeModeTabs {
    let tabs = item_tabs(creative)
        .map(|tab| {
            let items = tab
                .items
                .iter()
                .filter_map(|stack| stack.try_into_api().ok())
                .collect();
            mod_api::CreativeModeTabInfo::new(tab.id.clone(), items)
        })
        .collect();
    mod_api::CreativeModeTabs::new(creative.permissions(), tabs)
}
