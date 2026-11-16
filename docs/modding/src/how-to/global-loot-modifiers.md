# Global loot modifiers

Global loot modifiers are registered serializer types with data-enabled instances, not per-roll
listeners. A type with no instance file never runs. Native and wasm use the same interfaces.

Register a `GlobalLootModifierSerializer` in `Hooks::register`:

```rust,ignore
fn register(&self, registrar: &mut dyn mod_api::Registrar, _: &mod_api::RegistrationContext) {
    registrar.register_global_loot_modifier_serializer("bonus", BonusSerializer);
}
```

The resulting type id belongs to the mod's manifest namespace. Duplicate ids, invalid ids and ids
in another mod's namespace refuse registration. `minecraft:`, `sunrise:` and `neoforge:` are reserved.
The static serializer registry is `neoforge:global_loot_modifier_serializers`; it is not synced to clients.

## Preparing an instance

`GlobalLootModifierSerializer::decode` takes the instance's JSON and a borrowed `DataLoadContext`,
and returns `Result<Arc<dyn GlobalLootModifier>, LootModifierError>`. Validate custom fields there,
then retain typed immutable values. The load context exposes the owning registry/settings snapshot,
not a mutable game host. Report invalid fields with `LootModifierError::new(field_path, reason)`.

`LootModifierSettings::decode` parses the optional common fields:

- `priority`: a signed 32-bit integer, default `1000`; higher priorities run first, ties have no
  contractual order;
- `condition`: an inline loot condition or predicate id, absent for unconditional application.

A serializer can retain those settings and implement `GlobalLootModifier::priority` and `condition`
from them, returning `Ok(value)`. Both metadata methods are fallible so a refused wasm call cannot
silently turn into a default priority or an absent condition. The runtime captures them once during
decoding, not during rolls. The prepared
instance and its code store stay alive while a roll retains their generation, even after reload.

Instances are selected by files under `data/<namespace>/loot_modifiers/<path>.json`, for example:

```json
{
  "type": "example:bonus",
  "priority": 1100,
  "condition": {
    "type": "neoforge:loot_table_id",
    "loot_table_id": "minecraft:chests/simple_dungeon"
  }
}
```

## Applying an instance

`GlobalLootModifier::apply` takes a read-only `Host`, a borrowed `LootModifierContext` and the
preceding list as `Vec<LootStack>`. It returns the replacement list or a typed error. It cannot
write world state through this host. Keep implementations `Send + Sync` and retain no borrowed
context beyond the callback.

The list keeps signed counts and arbitrary component patches. A modifier may add, remove, replace
or reorder entries, including empty and over-maximum stacks. No second splitter or count clamp runs
between modifiers or after the pass. If application fails, later modifiers receive the preceding
list; draws already made from the roll random are not undone. A native panic disables the failing
prepared instance through the standard retained-callback policy.

`context.snapshot()` reads the current facts and the set-once queried root table id. Missing facts
stay missing. Its legacy `seed` field is zero: obtaining a snapshot does not draw a new seed.
`context.random()` borrows the actual selected roll source through `StructureRandom`, the existing
shared draw capability; do not create a separate seeded generator. `context.roll_raw(&table_id)`
rolls into an appendable list using the same parameters, selected random, visited-table guards and
data generation. It runs neither the modifier pass nor another stack splitter. A missing table
returns an error naming it; a visited table terminates with an empty list.

The server's root pass is after table functions and stack splitting, before caller consumption and
container shuffle. Nested tables and raw sub-rolls do not run it. `loot_drops` and
`container_loot_filled` are separate later moments, not aliases for global loot modifiers.
