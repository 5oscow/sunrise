# Asking a portal where it leads

Ask a portal block where an entity entering it would arrive, without moving anything, and, on the server, have the
exit portal built when none stands there yet. Two calls on `LevelRef` share one search with dimension travel.

| I want to | Use | Side |
|---|---|---|
| Know where a portal leads, changing nothing | `LevelRef::portal_destination` | Server |
| Reach the exit, building a nether portal when none exists | `LevelRef::find_or_create_portal_exit` | Server |

Both take the portal block's position, the direction the entity faces going in, and the entity's box as
`[width, height]`. They answer for an entity that is not a player, centred in the portal block, as an item or a mob
travels. Vanilla sends a player to an End portal's destination through the respawn search, which these calls do not run.

```rust
let size = [0.98, 0.98];
match level.find_or_create_portal_exit(host, portal_pos, Direction::North, size) {
    Ok(exit) => {
        // `exit.level` is the destination level, `exit.pos` the bottom centre of the box there and
        // `exit.yaw_turn` the degrees to add to the entity's yaw (0 or 90).
    }
    Err(PortalError::ChunksNotLoaded) => { /* retry on a later tick */ }
    Err(error) => { /* the other refusals are final for this call */ }
}
```

## What is answered

- **A nether portal:** the other of the Overworld and the Nether, the approximate exit (the coordinates scaled by the
  two dimensions' scales and clamped to the destination's world border), then the closest portal within 16 blocks
  into the Nether or 128 blocks out of it. `pos` and `yaw_turn` follow vanilla's placement in that portal's rectangle:
  the entity keeps its relative place, and the yaw turns 90 degrees when the two portals' axes differ.
  `exit_portal` is the lower corner of the portal found or built and `axis` its axis. The answer does not depend on
  `facing`; the direction is part of the call so a caller states how the entity enters.
- **An End portal:** into the End, the End's fixed spawn point; out of it, the Overworld's shared spawn. The
  obsidian platform is not rebuilt.
- **An End gateway:** the exit stored in the gateway, when it has one. A gateway that has not found its exit yet gives
  `NoExit`.

The read has none of travel's side effects: no platform, no portal ticket, no sound, and no hook fires.

## Refusals

| `PortalError` | Meaning |
|---|---|
| `NotAPortal` | The block at `portal_pos` is not a nether portal, End portal or End gateway. |
| `LevelMissing` | The handle is stale, or the destination level does not exist. |
| `EnteringNotAllowed` | The `allow_entering_nether_using_portals` game rule forbids the trip into the Nether. |
| `NoExit` | No exit portal was found. For `find_or_create_portal_exit`: none was found and none could be built (no room, or the world border). |
| `ChunksNotLoaded` | The destination level is not open, or a chunk the search or the build reads is not loaded. |
| `InTransaction` | `find_or_create_portal_exit` ran inside a transaction: a built portal cannot be rolled back. |

## Diverges from vanilla

Vanilla loads the chunks around the exit while it searches. These calls read loaded state only, in the level in place
or a parked one, and read nothing from the save. When the destination level is not open or a chunk within the search
radius (16 blocks, or 128 blocks, plus a chunk) is not loaded, they answer `ChunksNotLoaded`; keep the area
loaded, for example with a chunk ticket, and retry on a later tick. The search reads every chunk in that radius, so
a call out of the Nether looks at about 360 chunks.

`find_or_create_portal_exit` never opens a level either, and building needs the chunks around the exit to be
loaded too. The `portal_create` hook fires for a portal it builds, as it does for travel; it cannot veto the build.
