# Feature types

Register a `FeatureType` in `Hooks::register` to place features named by a configured feature's
`"type"` field. For example, `data/mymod/worldgen/configured_feature/tall_pillar.json` holding
`{"type": "mymod:pillar", "height": 4}` uses your registered `mymod:pillar` implementation:

```rust,ignore
{{#include ../../../examples/src/worldgen.rs:feature}}
```

This is vanilla's feature-type registry (`minecraft:worldgen/feature_type`, NeoForge's
`FEATURE_TYPE`); a configured feature is an entry of `minecraft:worldgen/feature`.

Use an ID in your mod's namespace. These stop startup with a message naming your mod: a
duplicate ID, an ID in another mod's namespace (vanilla's `minecraft:` included), an empty name,
an invalid config schema, and a feature type registered by a wasm mod (feature types run on the
worldgen workers, which wasm mods can't serve yet).

## Config

The second argument of `register_feature_type` is your config's schema: a flat record of
`DatapackField`s, the same one [datapack registries](../content-mods.md) use (`Bool`, `Int`,
`Float`, `Text`, `Identifier`, each with its bounds and an optional default). The game decodes
every configured feature of your type against it when the definition loads: an undeclared key, a
missing required key, a wrong type or a value out of bounds refuses that configured feature then,
naming it and the key, so a bad data pack never reaches generation. `type` is the game's own key.
`Reference` fields aren't accepted here; name another entry with `Identifier`.

`context.config()` is the decoded `DatapackEntry`: `config().get("height")` is a `DatapackValue`,
defaults filled in.

## Where your feature runs

A configured feature of your type places wherever the game places a feature, with no extra code:

| Route | What calls your `place` |
|---|---|
| Biome decoration | A placed feature in a biome's `features` list (or added by a biome modifier), on the worldgen workers, once per position its placement modifiers give |
| Nested features | A selector (`minecraft:random_selector`, `simple_random_selector`, `random_boolean_selector`) choosing it |
| Growth and bone meal | A sapling or grass whose configured feature it is |
| Commands | `/place feature mymod:tall_pillar` |
| Code | `LevelRef::place_feature(host, "mymod:tall_pillar", pos)` |

Vanilla's placement modifiers still choose the positions (`count`, `in_square`, `height_range`,
`biome` and the rest). Your feature runs at each position as soon as the last modifier gives it,
before the modifiers draw for the next position, in vanilla's depth-first order. A position given
twice is placed twice.

## Placing

`FeatureType::place(&self, context: &mut dyn FeatureContext, origin: [i32; 3])` returns:

- `Ok(true)` when the feature placed.
- `Ok(false)` when it didn't, for example a tree with no room. Blocks written before that are
  kept, as vanilla keeps them.
- `Err(FeatureError(reason))` for a failure.

Your writes are staged: `context.block` reads them back, and the level sees them, in order, only
when `place` answers `Ok`. A failure, a panic or running out of work discards all of them, so a
failed call leaves nothing behind. A failure or a panic also disables your type for the rest of
the run: each later placement of it fails by name without calling it.

There is no `Host` in a feature. Decoration runs on worker threads, so your type must be
`Send + Sync`, stateless and deterministic: draw only from `context.random()`, the same stream
the modifiers before you drew from.

| `FeatureContext` access | Purpose |
|---|---|
| `config` | The configured feature's decoded settings |
| `random` | The caller's `StructureRandom`: `next_int_bound`, `next_long`, `next_float`, `next_double`, `next_boolean` |
| `block`, `block_in_tag` | Read blocks and block tags, your own staged writes included |
| `height` | Heightmap heights, as the level has them before your writes |
| `set_block` | Write a `BlockState` with `BlockUpdateFlags`; `Feature.setBlock` uses `NEIGHBORS \| CLIENTS` |
| `block_entity` | Give a block you placed in this call its block entity's saved NBT |
| `state`, `state_with`, `is_air`, `metadata` | Resolve a registered `BlockKey` to a `BlockState`, and test states |
| `min_y`, `max_y` | The level's height bounds |

`set_block` answers whether the level will take the write: `false` outside the region it may
write (during decoration, the game's own decoration region around the chunk), outside the build
height, and for the state already there, as `Level.setBlock` answers. A read the level can't
answer is `None`. Neither ever loads a chunk. `block_entity` is `false` for a position you didn't
place a block at in this call, and for data whose `id` isn't that block's block entity.

Each context call is one unit of work. One root placement (a decoration position, a command, a
sapling) lends its mod features 65,536 units together, apart from the game's own feature budget.
Past them, calls answer `None` or `false` and the placement fails without your writes; the next
placement starts with the whole budget again.

Resolve block states once per placement, or keep the `BlockKey`s from registration, rather than
resolving names for every block.
