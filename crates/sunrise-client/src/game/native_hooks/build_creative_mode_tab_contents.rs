//! One demand-driven creative catalog per logical side, and the scoped edit event.
use super::*;
use crate::game::api_values::{TryIntoApi, TryIntoEngine};
use mod_api::{
    CreativeModeTabInfo, CreativeModeTabs, CreativeTabChange, CreativeTabEdit,
    CreativeTabEditError, CreativeTabList, CreativeTabPermissions, CreativeTabPlacement,
    CreativeTabReadError, CreativeTabVisibility,
};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use sunrise_items::creative_catalog::{
    Catalog, Holders, Kind, Parameters, TabContents, normalize_components,
};

use sunrise_items::items::Items;

pub(in crate::game) type Contents = Arc<[Arc<TabContents>]>;

struct BuildKey {
    features: Arc<[String]>,
    permissions: CreativeTabPermissions,
}

impl BuildKey {
    fn matches(&self, other: &Self) -> bool {
        self.permissions == other.permissions
            && (self.features == other.features
                || (other
                    .features
                    .iter()
                    .all(|flag| self.features.contains(flag))
                    && self
                        .features
                        .iter()
                        .all(|flag| other.features.contains(flag))))
    }
}

struct Published {
    key: BuildKey,
    lists: Contents,
    api: Option<Arc<CreativeModeTabs>>,
    holders: Arc<Holders>,
}

struct ActiveBuild {
    key: BuildKey,
    partial: Contents,
    edit: RefCell<Option<EditSession>>,
}

struct EditSession {
    token: CreativeTabEdit,
    contents: TabContents,
}

/// Replaced with the holder provider, never with tags or presentation. Thus the
/// cache key need not carry a registry pointer that changes on a tag-only update.
#[derive(Default)]
pub(in crate::game) struct CreativeCache {
    published: Option<Published>,
    active: Option<ActiveBuild>,
}

impl Game<'_> {
    pub(in crate::game) fn reset_creative_contents(&mut self) {
        self.creative_contents = CreativeCache::default();
        self.creative_screen.reset_contents();
    }

    /// Fire after one tab's generator with the live edit session installed.
    fn native_build_creative_mode_tab_contents(
        &mut self,
        tab: &str,
        features: Vec<String>,
        permissions: mod_api::CreativeTabPermissions,
        edit: mod_api::CreativeTabEdit,
    ) {
        self.fire_build_creative_mode_tab_contents(tab, features, permissions, edit);
    }

    pub(in crate::game) fn creative_permissions(&self) -> CreativeTabPermissions {
        if self.can_use_game_master_blocks() && self.settings.operator_items_tab {
            CreativeTabPermissions::Operator
        } else {
            CreativeTabPermissions::Normal
        }
    }

    pub(in crate::game) fn build_creative(
        &mut self,
        permissions: CreativeTabPermissions,
    ) -> Result<Contents, CreativeTabReadError> {
        // A connection may exist in configuration without a live world provider.
        if self.native_lifecycle.epoch().is_none() {
            return Err(CreativeTabReadError::Unavailable);
        }
        let key = BuildKey {
            features: self.enabled_feature_ids().clone(),
            permissions,
        };
        if let Some(active) = &self.creative_contents.active {
            if active.key.matches(&key) {
                return Ok(active.partial.clone());
            }
            return Err(CreativeTabReadError::Reentrant);
        }
        if let Some(published) = &self.creative_contents.published
            && published.key.matches(&key)
        {
            return Ok(published.lists.clone());
        }
        let holders = match &self.remote {
            Some(remote) => {
                sunrise_client::creative::live_holders(&remote.builtin, &remote.registries)
            }
            None => self
                .registry_sync()
                .ok_or(CreativeTabReadError::Unavailable)?
                .creative_holders(),
        };
        let items = match (&self.server_items, &self.world_tables) {
            (Some(items), _) => Arc::clone(items),
            (_, Some(tables)) => Arc::clone(&tables.items),
            _ => self.res.shared_items(),
        };
        let catalog = self.creative_screen.catalog().clone();
        let prior = self
            .creative_contents
            .published
            .as_ref()
            .map(|p| p.lists.clone());
        let listen = mod_api::__private::listened(mod_api::HookId::BuildCreativeModeTabContents);
        self.creative_contents.active = Some(ActiveBuild {
            key: BuildKey {
                features: key.features.clone(),
                permissions,
            },
            partial: Arc::from([]),
            edit: RefCell::new(None),
        });
        let lists = catalog.build(
            Parameters {
                features: &key.features,
                operator: permissions == CreativeTabPermissions::Operator,
                holders: &holders,
                items: &items,
            },
            prior.as_deref(),
            |tab, partial, current| {
                if listen {
                    static NEXT_EDIT: AtomicU64 = AtomicU64::new(0);
                    let token = mod_api::__private::creative_tab_edit(
                        NEXT_EDIT.fetch_add(1, Ordering::Relaxed),
                    );
                    let active = self
                        .creative_contents
                        .active
                        .as_mut()
                        .expect("active creative build");
                    active.partial = Arc::from(partial);
                    *active.edit.get_mut() = Some(EditSession {
                        token,
                        contents: std::mem::take(current),
                    });
                    self.native_build_creative_mode_tab_contents(
                        &tab.id,
                        key.features.to_vec(),
                        permissions,
                        token,
                    );
                    *current = self
                        .creative_contents
                        .active
                        .as_mut()
                        .expect("active creative build")
                        .edit
                        .get_mut()
                        .take()
                        .expect("active tab edit")
                        .contents;
                }
            },
        );
        self.creative_contents.active = None;
        self.creative_contents.published = Some(Published {
            key,
            lists: lists.clone(),
            api: None,
            holders: Arc::new(holders),
        });
        Ok(lists)
    }

    /// The screen uses the completed cache, with the same holder definitions for search text.
    pub(in crate::game) fn refresh_creative_tabs(&mut self) {
        let Ok(lists) = self.build_creative(self.creative_permissions()) else {
            return;
        };
        if let Some(published) = &self.creative_contents.published
            && Arc::ptr_eq(&lists, &published.lists)
        {
            self.creative_screen
                .publish(lists, published.holders.clone(), &self.pack.names);
        }
    }

    pub(in crate::game) fn creative_tabs(
        &mut self,
        permissions: CreativeTabPermissions,
    ) -> Result<Arc<CreativeModeTabs>, CreativeTabReadError> {
        let lists = self.build_creative(permissions)?;
        let catalog = self.creative_screen.catalog();
        if self.creative_contents.active.is_some() {
            return Ok(snapshot(catalog, permissions, &lists));
        }
        let published = self
            .creative_contents
            .published
            .as_mut()
            .expect("complete creative build");
        Ok(published
            .api
            .get_or_insert_with(|| snapshot(catalog, permissions, &lists))
            .clone())
    }

    /// The search tab's canonical display list (not the client-only UI additions).
    pub(in crate::game) fn creative_search_items(&mut self) -> Vec<ItemStack> {
        let Ok(lists) = self.build_creative(self.creative_permissions()) else {
            return Vec::new();
        };
        self.creative_screen
            .catalog()
            .tabs()
            .iter()
            .zip(&*lists)
            .find(|(tab, _)| tab.kind == Kind::Search)
            .map_or_else(Vec::new, |(_, contents)| contents.display.clone())
    }

    pub(in crate::game) fn creative_tab_entries(
        &self,
        edit: CreativeTabEdit,
        list: CreativeTabList,
    ) -> Result<Vec<ItemStack>, CreativeTabEditError> {
        self.edit_creative(edit, |contents| Ok(list_items(contents, list).clone()))
    }

    pub(in crate::game) fn creative_tab_change(
        &self,
        edit: CreativeTabEdit,
        change: CreativeTabChange,
    ) -> Result<(), CreativeTabEditError> {
        match change {
            CreativeTabChange::Insert {
                placement,
                stack,
                visibility,
            } => self.creative_tab_insert(edit, placement, stack, visibility),
            CreativeTabChange::Remove { stack, visibility } => {
                self.creative_tab_remove(edit, &stack, visibility)
            }
        }
    }

    fn creative_tab_insert(
        &self,
        edit: CreativeTabEdit,
        placement: CreativeTabPlacement,
        stack: mod_api::ItemStack,
        visibility: CreativeTabVisibility,
    ) -> Result<(), CreativeTabEditError> {
        self.edit_creative(edit, |contents| {
            if stack.count() != 1 {
                return Err(CreativeTabEditError::Count);
            }
            let items = self.items();
            let stack = edit_stack(&stack, items)?;
            let (anchor, after) = match &placement {
                CreativeTabPlacement::Before(anchor) => (Some(edit_stack(anchor, items)?), false),
                CreativeTabPlacement::After(anchor) => (Some(edit_stack(anchor, items)?), true),
                _ => (None, false),
            };
            for list in target_lists(visibility) {
                let entries = list_items(contents, *list);
                if entries
                    .iter()
                    .any(|entry| entry.same_item_same_components(&stack))
                {
                    return Err(CreativeTabEditError::Duplicate(*list));
                }
                let index = match &anchor {
                    Some(anchor) => {
                        entries
                            .iter()
                            .position(|entry| entry.same_item_same_components(anchor))
                            .ok_or(CreativeTabEditError::MissingAnchor(*list))?
                            + usize::from(after)
                    }
                    None if matches!(placement, CreativeTabPlacement::First) => 0,
                    None => entries.len(),
                };
                entries.insert(index, stack.clone());
            }
            Ok(())
        })
    }

    fn creative_tab_remove(
        &self,
        edit: CreativeTabEdit,
        stack: &mod_api::ItemStack,
        visibility: CreativeTabVisibility,
    ) -> Result<(), CreativeTabEditError> {
        self.edit_creative(edit, |contents| {
            let stack = edit_stack(stack, self.items())?;
            for list in target_lists(visibility) {
                list_items(contents, *list)
                    .retain(|entry| !entry.same_item_same_components(&stack));
            }
            Ok(())
        })
    }

    fn edit_creative<T>(
        &self,
        edit: CreativeTabEdit,
        apply: impl FnOnce(&mut TabContents) -> Result<T, CreativeTabEditError>,
    ) -> Result<T, CreativeTabEditError> {
        let Some(active) = &self.creative_contents.active else {
            return Err(CreativeTabEditError::Stale);
        };
        let mut session = active
            .edit
            .try_borrow_mut()
            .map_err(|_| CreativeTabEditError::Stale)?;
        let session = session
            .as_mut()
            .filter(|s| s.token == edit)
            .ok_or(CreativeTabEditError::Stale)?;
        apply(&mut session.contents)
    }
}

fn api_stacks(stacks: &[ItemStack]) -> Vec<mod_api::ItemStack> {
    stacks
        .iter()
        .filter_map(|item| item.try_into_api().ok())
        .collect()
}

fn snapshot(
    catalog: &Catalog,
    permissions: CreativeTabPermissions,
    lists: &[Arc<TabContents>],
) -> Arc<CreativeModeTabs> {
    let tabs = catalog
        .tabs()
        .iter()
        .zip(lists)
        .map(|(tab, stacks)| {
            CreativeModeTabInfo::new(
                tab.id.clone(),
                tab.kind == Kind::Category,
                api_stacks(&stacks.display),
                api_stacks(&stacks.search),
            )
        })
        .collect();
    Arc::new(CreativeModeTabs::new(permissions, tabs))
}

fn edit_stack(
    stack: &mod_api::ItemStack,
    items: &Items,
) -> Result<ItemStack, CreativeTabEditError> {
    // Counts on anchors/removals are ignored, including when received over Wire.
    let mut stack = stack.clone();
    stack.set_count(1).expect("one is not an empty stack");
    let mut stack = stack
        .try_into_engine(items)
        .map_err(CreativeTabEditError::Stack)?;
    normalize_components(&mut stack, items);
    Ok(stack)
}

fn target_lists(visibility: CreativeTabVisibility) -> &'static [CreativeTabList] {
    use CreativeTabList::{Parent, Search};
    match visibility {
        CreativeTabVisibility::Parent => &[Parent],
        CreativeTabVisibility::Search => &[Search],
        CreativeTabVisibility::Both => &[Parent, Search],
    }
}

fn list_items(contents: &mut TabContents, list: CreativeTabList) -> &mut Vec<ItemStack> {
    match list {
        CreativeTabList::Parent => &mut contents.display,
        CreativeTabList::Search => &mut contents.search,
    }
}

#[cfg(test)]
#[path = "build_creative_mode_tab_contents/tests.rs"]
mod tests;
