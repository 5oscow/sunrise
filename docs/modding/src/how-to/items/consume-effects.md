# Custom consume effects

Implement `Hooks::consume_effect` to handle a consumable's `on_consume_effects`
entry whose type is in your mod's namespace. The hook runs on the server thread
for the acting player when the item is consumed.

- The event provides the effect kind and its definition payload as a borrowed `Tag`.
- You can apply effects, play sounds, teleport the player and drop items through the host.
- An unclaimed kind does nothing.
- Consumable and death protection payloads are decoded once for item definitions
  and stack component patches. The event borrows the retained payload.
- Invalid payloads refuse loading rather than failing during consumption.

To apply an effect without consuming an item, use
[`LivingRef::apply_consume_effect`](consumable-effects.md).

## Food eligibility

`Hooks::can_eat` receives the eating `player` on both sides, with `stack = None`
for cake and candle cake. Read food through `event.player.food(host)` and effects
through `host.living(event.player.entity())`. On the client these are the local
player's copies of server-sent state. Return `Some(true)` to permit eating,
`Some(false)` to refuse it, or `None` to keep `vanilla_can_eat`. Always-eat
eligibility comes from the food component, not a separate event field.

