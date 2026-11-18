# Placing a block from code

`level.set_block(host, pos, state, flags)` writes a state without running `Block.setPlacedBy`.
These are all server operations on the level in place.

- `level.update_from_neighbour_shapes(host, pos, state)` is `Block.updateFromNeighbourShapes`: it returns
  the state fitted to the blocks around `pos` (a fence gets its connections, a stair its corner). Nothing
  is written; pass the result to `set_block`. Neighbours that are not loaded read as air.
- `level.set_placed_by(host, pos, stack)` runs `Block.setPlacedBy` on the block already at `pos`, with
  no placer. Pass `Some(&stack)` for a detached item value or `None` for an empty stack. A block can
  inspect the stack's components — for example a jukebox sets `has_record` when the stack's
  `minecraft:block_entity_data` contains `RecordItem` — but this call does not load that data or
  consume the item. It returns `Ok(false)` if the position is not writable, or a `LevelWriteError`
  for a stale or unavailable level or an invalid stack.
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
else of `BlockItem.place` runs: no survival or obstruction check, item placement sound, placement
game event or advancement. `place_block` and `update_from_neighbour_shapes` refuse with `LevelWriteError`:
`Stale` for a dead handle, `Unavailable` on a client or for a level that is not the one in place, and
`InvalidState` for a state the world lacks.

To place from a stack or saved data, write the state with `set_block`, then load the block entity data
if any, then call `set_placed_by` with the original stack (if any). `BlockItem.place` applies item data
before `Block.setPlacedBy`; the standalone call only runs the last step.
