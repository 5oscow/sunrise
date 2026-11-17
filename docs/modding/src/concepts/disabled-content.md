# Switching content off

Switching a mod off suspends its code, not its registered content. Blocks keep their appearance,
collision, loot and data-only behaviour. Where the content owner's Part subscribed to a world-writing
behaviour, the disabled block is inert rather than inheriting its vanilla template's implementation:

- `random_tick` skips the template tick. `tick_block` holds the scheduled occurrence before dispatch
  to any Part, including enabled Parts. Re-enabling the owner dispatches each held occurrence once,
  with its original deadline, priority and order.
- `update_shape` keeps the input state and `can_survive` answers true, so the template cannot
  replace the block or make it pop off.
- `use_item_on` and `use_without_item` pass without invoking the block template's interaction.
- `is_valid_bonemeal_target` refuses bone meal. For `on_tree_grow`, the soil under the trunk stays
  unchanged and the rest of the tree grows normally.
- `dispense_method` retains the original input stack and emits no dispense event unless an earlier
  enabled Part supplies a valid reply. A rejected reply does not restore template behaviour.
- `tool_modified_state` suppresses an unchanged template result, not a listener's replacement.
  The global `block_tool_modification` hook still hears the candidate for each attempted ability, and
  the tool can try its next ability if there is no result.
- Block entities belonging to the disabled mod do not tick, including through native tick loops.
  Their persistent state stays in the world and resumes when enabled.

Except for scheduled block ticks, these defaults occupy the disabled owner's usual place in Part
dispatch: earlier enabled Parts can answer first. A disabled mod's Part attached to another owner's
block does not make that block inert. Read-only queries (such as shapes, fluid state and loot) retain
their ordinary data/template fallback. These rules are the same on native and wasm.

Items and entities keep their registered data and default engine behaviour while their Part
callbacks are disabled. They are not globally frozen by this block-specific policy. In particular,
item use code is skipped but ordinary data-driven item behaviour remains; entity movement and
vanilla AI are not suspended. Do not switch a mod off as a way to pause its entities.
