//! Data-enabled, server-only global loot modifiers.
//!
//! Register a serializer once; data files under `loot_modifiers/` select its
//! prepared instances. A top-level roll runs them after its table functions and
//! stack splitter. Nested tables and raw sub-rolls never invoke this pass.
use crate::wire::{Reader, Wire, WireError, Writer};
use crate::{DataLoadContext, Host, Identifier, LootContext, LootStack, StructureRandom};
use serde_json::Value;
use std::sync::Arc;

const DEFAULT_PRIORITY: i32 = 1000;

#[path = "loot_modifiers/owned.rs"]
mod owned;
pub(crate) use owned::owned_serializer;

/// Frozen serializer declarations at the hidden game integration boundary.
// hidden: the game builds its static serializer registry once after registration.
#[doc(hidden)]
pub fn global_loot_modifier_serializer_declarations()
-> Vec<(String, Arc<dyn GlobalLootModifierSerializer>)> {
    crate::parts::loot_modifier_serializer_list()
}

/// A serializer or application failure, reported with its instance and type ids.
#[derive(Clone, Debug, PartialEq, Eq, sunrise_mod_api_macros::Wire)]
#[non_exhaustive]
pub struct LootModifierError {
    /// JSON field path at decode time, or the failed operation at apply time.
    pub field: String,
    /// Explanation suitable for the server log.
    pub reason: String,
}
impl LootModifierError {
    /// Refuse one field or operation without panicking.
    pub fn new(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
        }
    }
}
impl std::fmt::Display for LootModifierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}
impl std::error::Error for LootModifierError {}
impl From<(Identifier, String)> for LootModifierError {
    fn from((source, reason): (Identifier, String)) -> Self {
        Self::new("$", format!("{source}: {reason}"))
    }
}

/// The optional base fields shared by modifier codecs (`LootModifier.codecStart`).
#[derive(Clone, Debug, sunrise_mod_api_macros::Wire)]
#[non_exhaustive]
pub struct LootModifierSettings {
    /// Higher priorities run first. Ties have no contractual order.
    pub priority: i32,
    /// Inline loot condition or predicate id; absent means unconditional.
    pub condition: Option<Value>,
}
impl Default for LootModifierSettings {
    fn default() -> Self {
        Self {
            priority: DEFAULT_PRIORITY,
            condition: None,
        }
    }
}
impl LootModifierSettings {
    /// Decode the common fields once, alongside a serializer's own typed fields.
    /// Unknown fields are left to that serializer. Predicate contents and custom
    /// condition kinds are resolved by the owning loot data generation.
    pub fn decode(data: &Value) -> Result<Self, LootModifierError> {
        let object = data
            .as_object()
            .ok_or_else(|| LootModifierError::new("$", "expected a modifier object"))?;
        let priority = match object.get("priority") {
            None => DEFAULT_PRIORITY,
            Some(value) => value
                .as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(|| {
                    LootModifierError::new("priority", "expected a signed 32-bit integer")
                })?,
        };
        let condition = object
            .get("condition")
            .map(|condition| match condition {
                Value::Object(_) => Ok(condition.clone()),
                Value::String(id) => Identifier::parse(id)
                    .map(|id| Value::String(id.to_string()))
                    .map_err(|error| LootModifierError::new("condition", error.to_string())),
                _ => Err(LootModifierError::new(
                    "condition",
                    "expected an inline loot condition or predicate id",
                )),
            })
            .transpose()?;
        Ok(Self {
            priority,
            condition,
        })
    }
}

/// A type in the static, unsynced `neoforge:global_loot_modifier_serializers`
/// registry. Register through `Registrar::register_global_loot_modifier_serializer`.
/// Decode validates custom fields once and retains typed immutable state. A bad
/// file is diagnosed and skipped; it does not suppress other valid instances.
pub trait GlobalLootModifierSerializer: Send + Sync + 'static {
    /// Prepare one instance for this owning data load. The instance retains its
    /// implementation and code store while an active roll uses its generation.
    fn decode(
        &self,
        data: &Value,
        load: &dyn DataLoadContext,
    ) -> Result<Arc<dyn GlobalLootModifier>, LootModifierError>;
}

crate::wire::wire_handlers::handler!(
    dyn GlobalLootModifierSerializer,
    "GlobalLootModifierSerializer"
);

/// A prepared instance (`IGlobalLootModifier`), enabled by a data file, not a hook
/// listener. Priority and condition are captured once when decoding completes;
/// a metadata failure, including a refused wasm call, refuses that preparation.
pub trait GlobalLootModifier: Send + Sync + 'static {
    /// Higher priorities run first; the default is `IGlobalLootModifier.DEFAULT_PRIORITY`.
    fn priority(&self) -> Result<i32, LootModifierError> {
        Ok(DEFAULT_PRIORITY)
    }
    /// The optional base-class loot condition, evaluated with the actual roll
    /// context and random before application. A string references a predicate.
    fn condition(&self) -> Result<Option<Value>, LootModifierError> {
        Ok(None)
    }
    /// Replace the preceding modifier's list. Signed/empty and over-max stacks
    /// remain intact between modifiers; the boundary does not split, clamp or
    /// filter the result again. An error rejects this candidate and later
    /// modifiers continue with the preceding list. RNG draws are not rolled back.
    fn apply(
        &self,
        host: &dyn Host,
        context: &mut dyn LootModifierContext,
        generated: Vec<LootStack>,
    ) -> Result<Vec<LootStack>, LootModifierError>;
}

crate::wire::wire_handlers::handler!(dyn GlobalLootModifier, "GlobalLootModifier");

/// A roll-scoped resource, identical for native and wasm instances. It lends the
/// selected roll random, not a level random or an independently seeded stream.
/// A mod cannot retain this context beyond the callback.
pub trait LootModifierContext {
    /// Current optional facts and the set-once queried root id. The snapshot's
    /// legacy `seed` field is zero: reading facts never draws a replacement seed.
    /// Use [`Self::random`] for all modifier randomness.
    fn snapshot(&self) -> LootContext;
    /// Borrow the selected roll's actual random through the shared draw capability.
    /// Draws advance that source in order, including draws made by raw sub-rolls.
    fn random(&mut self) -> &mut dyn StructureRandom;
    /// Appendable raw output, with the same parameters, random, visited guards
    /// and data generation. This does not run modifiers or another stack splitter.
    /// A missing table refuses by id; a visited table terminates as an empty roll.
    fn roll_raw(&mut self, table: &Identifier) -> Result<Vec<LootStack>, LootModifierError>;
}

#[cfg(test)]
#[path = "loot_modifiers/tests.rs"]
mod tests;
