# Placing a world-generation feature

`host.place_feature` and `host.place_placed_feature` put a world-generation feature into the level
through the placement `/place feature` uses, and answer with what the feature reports. A tree grower, a
structure tool or a worldgen preview calls them directly instead of going through `run_command`.

```rust,ignore
// An entry of minecraft:worldgen/feature: a feature type with its settings.
match host.place_feature("minecraft:oak", [120, 65, -40]) {
    Ok(true) => { /* the feature reports it placed something */ }
    Ok(false) => { /* it reports nothing placed; blocks may still have been written */ }
    Err(reason) => { /* nothing was placed; `reason` says why */ }
}

// An entry of minecraft:worldgen/placed_feature: a feature with placement modifiers.
let any = host.place_placed_feature("minecraft:oak_checked", [120, 65, -40]);
```

Side: server. The call acts on the level the server is ticking, as `edit_blocks` and `place_template`
do, and refuses a client.

## What each call places

- `place_feature(id, at)` places the feature `id` with `at` as its origin. What the origin means is the
  feature type's own: a tree's base, an ore blob's centre. The feature draws from the level's random.
- `place_placed_feature(id, at)` runs the placed feature's modifiers in order from `at`, each turning
  one position into zero or more, depth first, and places the feature at every position that comes
  out. It answers `Ok(true)` when any of those placements did. With no position left it answers
  `Ok(false)`. There is no biome check from code, so a biome filter modifier passes everywhere.

## The answer

`Ok(false)` is the feature's own report, not a promise that the level is unchanged: some feature types
write blocks before they decide they failed. A mod that needs the level untouched on failure reads and
keeps what it needs before placing. Blocks reach players through the level's ordinary block changes; the
call sends nothing itself.

A feature can write beyond the origin's chunk, as far as its size allows. The call checks only the 3 by
3 chunks around `at`, as `/place feature` does.

## Refusals

`Err(String)` says why nothing was placed:

| Reason | What was wrong |
|---|---|
| a client of a server places nothing | the call ran on a client |
| chunk (x, z) around the origin is not loaded | one of the 3 by 3 chunks around `at` is not loaded |
| the game has no feature or placed feature | the id is not in the registry, or the game has no feature resources |

Nothing here panics on what a mod supplies: an id that is not an id at all is an unknown id.
