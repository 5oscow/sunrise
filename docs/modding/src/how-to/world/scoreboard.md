# Read and write the scoreboard

A server mod reaches the world's scoreboard through objective handles, as it reaches a custom
boss bar. `host.objective(name)` finds an objective, `host.objectives()` lists them,
`host.add_objective(name, criteria, display_name, render_type)` adds one, and
`display_slot_objective` and `set_display_slot` read and change what each display slot shows.
The handle, an `ObjectiveRef`, reads and writes the rest: the criteria, display name, render
type, number format, display slots and the scores.

```rust,ignore
let kills = host.add_objective("kills", "dummy", Text::literal("Kills"), ObjectiveRenderType::Integer)?;
kills.add_score(host, ScoreHolder::Player(player), 1)?;
host.set_display_slot(DisplaySlot::Sidebar, Some(kills));
```

The saved board is loaded before the server-started hooks run, so a mod that reads at startup
finds its saved objectives.

## Same board, same write

A mod's write is the write `/scoreboard` makes on the server's board. Clients are sent the
change by the board's usual per-tick sync, `scoreboard.dat` holds it, and selectors
(`scores=`, `team=`) see it. A criteria that follows players (`deathCount`, statistics) keeps
updating as before. A score is sent to clients only while its objective is shown in a display
slot. Locks are never sent.

## Holders

A score holder is a `ScoreHolder`:

- `Player(player)` is an online player, keyed by their name.
- `Entity(entity)` is any entity. A player's entity is that player (the same key as
  `Player`); another entity is keyed by its UUID.
- `Name(string)` is any string, as vanilla's `ScoreHolder.forNameOnly`: offline players, fake
  holders like `#global`, display-source keys. The board takes every string. A name starting
  with `#` is stored and synced but hidden from the sidebar.

## Stale handles

An objective's handle holds the world's open and an identity the board gave the objective when
it was added. After the objective is removed (by `remove` or by `/scoreboard objectives remove`)
the handle stays stale for good, also when an objective of the same name is added again: reads
answer `None` or `ObjectiveError::Stale`, and writes change nothing. Ask the host for the
objective by name again to get a handle to the new one.

## Writes

`set_score` and `add_score` create the score at 0 first (locked, as in vanilla), tell clients
only when something changed, and wrap on overflow. They refuse a read-only criteria (`health`,
`food`, `air`, `armor`, `xp`, `level`) with `ObjectiveError::ReadOnlyCriteria`, as the command
does: only the server's own updates write those. A holder that is not on the server (an offline
player's entity) is `ObjectiveError::UnknownHolder`, which is not a stale handle: the objective
is fine, and the other score calls answer `None` or `false` for such a holder. `reset_score` removes one holder's score, and
`set_score_locked` changes the lock `/trigger` uses.

Inside `host.transaction` the writes apply at once, and an abort puts the board back as it was
when the transaction first wrote it. The board is copied once, when the first scoreboard write
of the transaction happens, and not at all in a transaction that writes none.

`scores()` lists in the board's own order. Vanilla's order is a hash map's, so do not rely on
one.

## On a client

`ClientHost::client_scoreboard()` is what this client has been sent: the objectives shown in a
display slot, with their slots and scores, as plain values. A client has no handles and
cannot write. A text the API cannot hold (nested deeper than 512 component nodes) refuses the
whole snapshot with `TextFormError::TooDeep`; nothing is flattened or skipped.
