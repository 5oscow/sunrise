# Custom consume effects

Implement `Hooks::consume_effect` to handle a consumable's `on_consume_effects`
entry whose type is in your mod's namespace. The hook runs on the server thread
for the consuming player or mob. Entries run in the effective consumable's list
order, interleaved with built-in effects; stack component patches count.

- The event provides the effect kind and its definition payload as a borrowed `Tag`.
- `event.stack` is a read-only snapshot before the stack shrinks, components included.
- `event.user` is the consuming entity, including a mob, not necessarily the acting player.
- You can apply effects, play sounds, teleport entities and drop items through the host.
- An unclaimed kind does nothing.
- Consumable and death protection payloads are decoded once for item definitions
  and stack component patches. The event borrows the retained payload. Player death
  protection uses this hook; mob death protection does not dispatch it.
- Invalid payloads refuse loading rather than failing during consumption.

To apply an effect without consuming an item, use
[`LivingRef::apply_consume_effect`](consumable-effects.md).
