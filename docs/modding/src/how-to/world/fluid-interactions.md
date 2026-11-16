# Fluid interactions

What a liquid block turns into beside another fluid or block: lava meeting water becomes
obsidian or cobblestone, a mod's honey meeting lava becomes its own stone. Vanilla's two
lava rules and a mod's are one table per source fluid, the shape of NeoForge's
`FluidInteractionRegistry`.

## Declaring one

In code, at registration:

```rust,ignore
let lava = registrar.fluid_key("minecraft:lava");
let honey = registrar.fluid_key("ruby:honey");
// `slag_state` is a `BlockState` of this mod's block (`BlockState::of`).
registrar.fluid_interaction(FluidInteraction::new(
    lava,
    FluidNeighbour::Fluid(honey),
    FluidBecomes::always(slag_state),
));
```

Or as one file per interaction, `data/<mod id>/sunrise/fluid_interaction/<name>.json`, loaded
with the mod's content (as `data/<mod id>/sunrise/fluid/` is) and not reloaded with data packs:

```json
{
  "source": "minecraft:lava",
  "neighbour": { "fluid": "ruby:honey" },
  "below": { "block_tag": "ruby:cold_floor" },
  "becomes": { "source": "ruby:slag", "flowing": "ruby:slag[age=1]" }
}
```

- `source` is the fluid whose block interacts; source and flowing form are one fluid. It may be
  another mod's fluid or `minecraft:lava`, so "lava next to my fluid becomes my stone" is yours
  to say.
- `neighbour` is one of `fluid`, `fluid_tag`, `block` or `block_tag`. A fluid is read from the
  neighbour cell's fluid state, so a waterlogged block counts as water. The neighbours are above,
  north, south, west and east, never below.
- `below` is optional: the block (`block`) or block tag (`block_tag`) under the liquid.
- `becomes` gives the block state a source cell becomes (`source`) and a flowing cell becomes
  (`flowing`), written as `/setblock` writes a state. A state left out is no interaction for
  that state of the cell.

An id that names no fluid or block refuses the world at load, by name. A tag is read again on every
data pack reload, so a pack that edits `minecraft:water` changes what lava meets, as in vanilla.

## What runs

When a liquid block is placed (including by spreading into a cell) and when a neighbour of it
changes, the first match over the neighbours, then over the interactions, wins; at most one
runs. Vanilla's two rules come first (lava beside water: obsidian for a source, cobblestone for
flowing lava; lava over soul soil beside blue ice: basalt). After them come native mods'
interactions in load order, then content mods' files in load order and file name. A fluid with no
interaction pays nothing.

The result goes through the [`fluid_place_block`](../../hooks.md) hook, which may veto or replace
it, then level event 1501 plays the fizz, and the liquid stops spreading. A veto writes nothing
and still fizzes and stops the spread.

## Diverges from NeoForge

- NeoForge runs its registry only on `neighborChanged`; `onPlace`, which covers a fluid spreading
  into a cell, keeps vanilla's hard-coded lava check. So there a mod's interaction waits for a
  later neighbour update. Sunrise runs the table at both moments.
- NeoForge's `FluidPlaceBlockEvent` ignores cancellation; Sunrise's `fluid_place_block` can
  veto, as it already does.

Not offered: a code predicate per check (NeoForge's `HasFluidInteraction`). The table covers
fluid, block and tag neighbours and the block below; a predicate would be a `FluidPart` method.
Lava flowing down into water making stone (`LavaFluid.spreadTo`) is a different vanilla rule and
stays as it is.
