# Ticking a registered mob effect

A content mod's effects can run server-side behaviour through two hooks on `Hooks`:

- `should_apply_effect_tick_this_tick` receives the resolved `effect` key, remaining
  duration (`tick_count`, or the holder's tick count for an infinite effect), and
  amplifier. Set `event.apply = true` to run the effect now; it starts false.
- `apply_effect_tick` runs only when the first hook says to apply. It receives the
  holder's `LivingRef`, dimension, health, maximum health and optional `PlayerFood`.
  Queue `heal`, `hurt`, `eat` or `add_exhaustion` changes through the event. These
  apply after the listener returns. Clear `event.keep` to end the effect immediately,
  before its duration counts down.

Both hooks are version 2. Only the mod that declared the effect's registry entry
receives them. Another mod requesting that effect's `EffectKey` does not acquire
its tick behaviour. A mod declaring several effects selects between them using
`event.effect`; resolve its keys during registration with `r.effect_key(id)`.

## Reading the holder

The ticking holder may be borrowed out of the read-only Host, so do not depend on
resolving its live entity storage through the handle during this callback. Use the
provided health and food snapshots and `event.has_effect(key)` instead. The
`active_effects: &[EffectKey]` snapshot reflects the holder's active list at this
point in the pass, including changes earlier in that pass. It is borrowed only
for the call: copy a key if you need to keep it.

For example, an effect that heals every 80 ticks can set `apply` when
`event.tick_count % 80 == 0`, then skip healing when
`event.has_effect(regeneration_key)` or when a player's saturation is positive.
A mob has `event.food == None` and can still heal.

The level remains readable. Obtain it with `host.level(event.dimension)` and use
`level.game_rule(host, "minecraft:natural_health_regeneration")` to read vanilla's
natural-regeneration rule. This read works while a mob's entity storage is
borrowed for its tick.

`event.add_exhaustion(amount)` follows vanilla's `FoodData.addExhaustion` on players:
it adds the amount and caps the result at 40, without a lower clamp. Negative
amounts therefore subtract exhaustion. To remove at most 4, read the current food
snapshot and add `-food.exhaustion.min(4.0)` only when exhaustion is positive. Food
changes have no effect on a mob without food data.

## Native and wasm

The same hooks, resolved keys, snapshots and queued changes cross the shared Wire
boundary for both targets. No engine-owned holder storage crosses it. A wasm
module requiring an unavailable import is refused while linking, naming the
module and missing symbol; it is not loaded with a silently disabled capability.
