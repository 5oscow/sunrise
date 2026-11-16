# Feature types

Register a `FeatureType` in `Hooks::register` to place features named by a configured feature's
`"type"` field. For example, `data/mymod/worldgen/configured_feature/tall_pillar.json` holding
`{"type": "mymod:pillar", "height": 4}` uses your registered `mymod:pillar` implementation:

```rust,ignore
{{#include ../../../examples/src/worldgen.rs:feature}}
```

Use an ID in your mod's namespace. A duplicate ID, an ID in another mod's namespace (vanilla's
`minecraft:` included), or an empty name stops startup with a message naming your mod.

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
- `Err(FeatureError(reason))` for a failure. The placement stops there, and the game reports it
  with your feature's id.

There is no `Host` in a feature. Decoration runs on worker threads, so your type must be
`Send + Sync`, stateless and deterministic: draw only from `context.random()`, the same stream
the modifiers before you drew from.

| `FeatureContext` access | Purpose |
|---|---|
| `config` | The configured feature's JSON object (its `type` and settings), converted to an `Arc<Tag>` once when it loads |
| `random` | The caller's `StructureRandom`: `next_int_bound`, `next_long`, `next_float`, `next_double`, `next_boolean` |
| `block`, `block_in_tag`, `height` | Read blocks, block tags and heightmap heights |
| `set_block` | Write a `BlockState` with `BlockUpdateFlags`; `Feature.setBlock` uses `NEIGHBORS \| CLIENTS` |
| `block_entity` | Give the block just placed its block entity's saved NBT |
| `state`, `state_with`, `is_air`, `metadata` | Resolve a registered `BlockKey` to a `BlockState`, and test states |
| `min_y`, `max_y` | The level's height bounds |

A read the level can't answer is `None`, and a write it refuses is `false`. During decoration the
readable and writable area is the game's own decoration region around the chunk, as for vanilla's
features. Neither ever loads a chunk.

Resolve block states once per placement, or keep the `BlockKey`s from registration, rather than
resolving names for every block.

A config the feature can't use is best refused in `place` with a `FeatureError`. JSON that can't
be converted to NBT (a nested `null`) refuses the configured feature when it loads.
