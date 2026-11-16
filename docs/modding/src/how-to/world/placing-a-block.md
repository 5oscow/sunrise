# Placing a block from code

`level.set_block(host, pos, state, flags)` writes a state and nothing more. A block placed by an item
does two things more, and a mod placing a block from code can ask for each of them. All three are server
operations on the level in place.

- `level.update_from_neighbour_shapes(host, pos, state)` is `Block.updateFromNeighbourShapes`: it returns
  the state fitted to the blocks around `pos` (a fence gets its connections, a stair its corner). Nothing
  is written; pass the result to `set_block`. Neighbours that are not loaded read as air.
- `level.place_block(host, pos, state, update_neighbour_shapes)` writes `state` with a placement's flags
  (neighbours and clients told, and unless `update_neighbour_shapes` is false their shapes updated) and
  then runs `Block.setPlacedBy`, so a bed gets its head, a door its upper half and a tall plant its top.
  It returns `Ok(true)` when the block was written, and `Ok(false)` when vanilla refused the write
  (an unchanged state, an unloaded or out-of-height position).

```rust,ignore
let fitted = level.update_from_neighbour_shapes(host, pos, door)?;
level.place_block(host, pos, fitted, true)?;
```

`place_block` is `player.place_block` without a player: `setPlacedBy` gets no placer and no item. Nothing
else of `BlockItem.place` runs: no survival or obstruction check, no sound, no game event, no
advancement. Both refuse with `LevelWriteError`: `Stale` for a dead handle, `Unavailable` on a client or
for a level that is not the one in place, and `InvalidState` for a state the world lacks.
