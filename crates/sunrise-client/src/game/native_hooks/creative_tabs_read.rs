//! `ItemsClientHost::creative_tabs` and `ClientRenderClientHost::item_uses_block_light`: the
//! creative tabs' contents and an item model's lighting, read by a client mod.
use super::*;
use crate::game::api_values::{TryIntoApi, engine_display_context};
use sunrise_client::cit::Hand as CitHand;

impl Game<'_> {
    /// The creative inventory's item tabs with their stacks, and the `hasPermissions` they were
    /// last built with.
    pub(in crate::game) fn creative_tabs(&self) -> mod_api::CreativeModeTabs {
        match self.preview_creative() {
            Some((tabs, _)) => creative_tabs_of_tabs(&tabs, false),
            None => creative_tabs_of_tabs(
                &self.creative_screen.tabs,
                self.creative_screen.permissions(),
            ),
        }
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
        self.resolve_item_in_level(stack, display, None, None, 0, CitHand::Main)
            .uses_block_light()
    }
}

/// The tabs `creative_tabs` lists: the item tabs, not the search, saved toolbars or survival
/// inventory tabs, in `CreativeModeTabs.tabs()` order.
#[cfg(test)]
pub(super) fn item_tabs(
    creative: &sunrise_client::creative::Creative,
) -> impl Iterator<Item = &sunrise_client::creative::Tab> {
    category_tabs(&creative.tabs)
}

fn category_tabs(
    tabs: &[sunrise_client::creative::Tab],
) -> impl Iterator<Item = &sunrise_client::creative::Tab> {
    tabs.iter()
        .filter(|tab| !tab.search && !tab.inventory && !tab.hotbar)
}

fn creative_tabs_of_tabs(
    tabs: &[sunrise_client::creative::Tab],
    permissions: bool,
) -> mod_api::CreativeModeTabs {
    let tabs = category_tabs(tabs)
        .map(|tab| {
            let items = tab
                .items
                .iter()
                .filter_map(|stack| stack.try_into_api().ok())
                .collect();
            mod_api::CreativeModeTabInfo::new(tab.id.clone(), items)
        })
        .collect();
    mod_api::CreativeModeTabs::new(permissions, tabs)
}
