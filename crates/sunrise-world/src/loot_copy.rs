//! Shared loot source snapshots and component-copy evaluation.
use super::LootStack;
use serde_json::Value;
use std::borrow::Cow;
use std::{cell::RefCell, sync::Arc};
use sunrise_core::nbt::{Compound, Tag};

/// Where a loot roll happens, as a mod's own loot kinds see it:
/// `LootContextParams.ORIGIN`, `LootContextParams.BLOCK_ENTITY`, and what
/// vanilla's own conditions read of `LootContext.getLevel` (`TimeCheck`:
/// the time; `WeatherCheck`: the rain and the thunder). The game sets it
/// around a roll with [`with_native_loot_place`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LootPlace {
    /// `ORIGIN`: a broken block's centre, a dead mob's position.
    pub origin: Option<[f64; 3]>,
    /// `BLOCK_ENTITY`: the broken block's entity as it was before the
    /// break, saved with its id and position (`saveWithFullMetadata`); a
    /// mod's block entity carries its own data and its `Items`.
    pub block_entity: Option<Tag>,
    /// The block entity's collected components, separate from its saved NBT.
    pub block_components: Option<Compound>,
    /// The level the roll is in.
    pub level: Option<LootLevel>,
}

/// The level a loot roll is in, as a mod's own loot kinds see it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LootLevel {
    /// The dimension's id (`minecraft:overworld`).
    pub dimension: String,
    /// `Level.getGameTime`.
    pub game_time: i64,
    /// `Level.getDayTime`: it counts on from day to day, so the time of
    /// day is `day_time % 24000` (night from 13000 to 23000).
    pub day_time: i64,
    /// `Level.isRaining`.
    pub raining: bool,
    /// `Level.isThundering`.
    pub thundering: bool,
}

thread_local! {
    static NATIVE_LOOT_PLACE: RefCell<Option<Arc<LootPlace>>> = const { RefCell::new(None) };
}

/// Runs `roll` with `place` as where its loot rolls happen: every
/// [`crate::loot::LootContext`] made inside it, by either engine, carries it. The
/// place before is back when `roll` returns, so these nest.
pub fn with_native_loot_place<T>(place: LootPlace, roll: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<LootPlace>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let before = self.0.take();
            NATIVE_LOOT_PLACE.with(|p| *p.borrow_mut() = before);
        }
    }
    let _restore = Restore(NATIVE_LOOT_PLACE.with(|p| p.replace(Some(Arc::new(place)))));
    roll()
}

/// The place [`with_native_loot_place`] set for the rolls now running, if any.
pub fn current_native_loot_place() -> Option<Arc<LootPlace>> {
    NATIVE_LOOT_PLACE.with(|p| p.borrow().clone())
}

/// Source captured before a block entity is removed. Copy policy belongs to the table.
#[derive(Clone, Debug)]
pub struct BlockSource {
    pub nbt: Tag,
    pub components: Compound,
}

/// Lend a removed block's retained source without losing an outer roll's origin or level.
pub fn with_block_source<T>(source: Option<&BlockSource>, roll: impl FnOnce() -> T) -> T {
    let Some(source) = source else { return roll() };
    let mut place = current_native_loot_place()
        .as_deref()
        .cloned()
        .unwrap_or_default();
    place.block_entity = Some(source.nbt.clone());
    place.block_components = Some(source.components.clone());
    with_native_loot_place(place, roll)
}

/// Copy only values the selected source actually has. Include and exclude intersect;
/// exclusion wins, and an absent source leaves the candidate untouched.
pub fn copy_components<'a>(
    function: &Value,
    source: impl FnOnce(&str) -> Option<Cow<'a, Compound>>,
    mut apply: impl FnMut(String, &Tag),
) {
    let Some(source) = function["source"].as_str().and_then(source) else {
        return;
    };
    let includes = function.get("include").and_then(Value::as_array);
    let excludes = function.get("exclude").and_then(Value::as_array);
    for (id, value) in source.iter() {
        let id = id.to_string_lossy();
        let contains = |list: &[Value]| {
            list.iter().filter_map(Value::as_str).any(|entry| {
                if entry.contains(':') {
                    entry == id
                } else {
                    id.strip_prefix("minecraft:") == Some(entry)
                }
            })
        };
        if includes.is_some_and(|list| !contains(list))
            || excludes.is_some_and(|list| contains(list))
        {
            continue;
        }
        apply(id, value);
    }
}

/// Preserve the exact loaded prototype, including native default-component changes.
pub fn component_defaults(
    defaults: &crate::item_defaults::ItemDefaults,
) -> Result<Arc<std::collections::HashMap<String, Compound>>, String> {
    defaults
        .entries()
        .iter()
        .map(|(id, value)| {
            let object = value
                .as_object()
                .ok_or_else(|| format!("{id}: component prototype is not an object"))?;
            Ok((
                id.clone(),
                sunrise_core::json_nbt::json_to_compound(object)
                    .map_err(|error| error.to_string())?,
            ))
        })
        .collect::<Result<_, String>>()
        .map(Arc::new)
}

impl super::LootData {
    pub fn component_defaults(&self) -> &Arc<std::collections::HashMap<String, Compound>> {
        &self.component_defaults
    }
}

impl<R: sunrise_core::random::RandomSource> super::Context<'_, R> {
    pub(super) fn copy_source_components(&self, function: &Value, stack: &mut LootStack) {
        let place = current_native_loot_place();
        copy_components(
            function,
            |source| match source {
                "block_entity" => place.as_ref()?.block_components.as_ref().map(Cow::Borrowed),
                "tool" => self.query.tool.as_ref().map(|tool| {
                    Cow::Owned(stack_components(
                        tool,
                        self.data.component_defaults.get(&tool.item),
                    ))
                }),
                target => self
                    .target(target)
                    .map(|entity| Cow::Borrowed(&entity.components)),
            },
            |id, value| {
                match (id.as_str(), value) {
                    ("minecraft:damage", Tag::Int(damage)) => stack.damage = *damage,
                    ("minecraft:enchantments", Tag::Compound(levels)) => {
                        stack.enchantments = levels
                            .iter()
                            .filter_map(|(key, value)| {
                                if let Tag::Int(level) = value {
                                    Some((key.to_string_lossy(), *level))
                                } else {
                                    None
                                }
                            })
                            .collect();
                    }
                    _ => {}
                }
                stack.components.push((id, Some(value.clone())));
            },
        );
    }
}

/// Apply a saved component patch to its loaded prototype, including removals.
pub fn components_with_patch(prototype: Option<&Compound>, patch: Option<&Tag>) -> Compound {
    let mut components = prototype.cloned().unwrap_or_default();
    if let Some(Tag::Compound(patch)) = patch {
        for (id, value) in patch {
            let name = id.to_string_lossy();
            if let Some(base) = name.strip_prefix('!') {
                components.remove(&base.into());
            } else {
                components.insert(id.clone(), value.clone());
            }
        }
    }
    components
}

/// A tool's effective components: prototype plus its ordered patch and live scalars.
pub fn stack_components(stack: &LootStack, prototype: Option<&Compound>) -> Compound {
    let mut components = prototype.cloned().unwrap_or_default();
    if stack.damage != 0 || components.contains_key(&"minecraft:damage".into()) {
        components.insert("minecraft:damage".into(), Tag::Int(stack.damage));
    }
    if !stack.enchantments.is_empty() {
        components.insert(
            "minecraft:enchantments".into(),
            Tag::Compound(
                stack
                    .enchantments
                    .iter()
                    .map(|(id, level)| (id.as_str().into(), Tag::Int(*level)))
                    .collect(),
            ),
        );
    }
    for (id, value) in &stack.names {
        if let Some(value) = super::json_tag(value) {
            components.insert(id.as_str().into(), value);
        }
    }
    for (id, value) in &stack.components {
        if let Some(value) = value {
            components.insert(id.as_str().into(), value.clone());
        } else {
            components.remove(&id.as_str().into());
        }
    }
    components
}
