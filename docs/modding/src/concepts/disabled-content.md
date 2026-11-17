# Switching content off

Switching a mod off suspends its code, not its registered content. Blocks keep their appearance,
collision, loot and data-only behaviour. Where the content owner's Part subscribed to a world-writing
behaviour, the disabled block is inert rather than inheriting its vanilla template's implementation:

- `random_tick` and `tick_block` skip the template tick. Scheduled ticks retain their original deadline,
  priority and order until the mod is enabled again; they are not discarded.
- `update_shape` keeps the input state and `can_survive` answers true, so the template cannot
  replace the block or make it pop off.
- `use_item_on` and `use_without_item` pass without invoking the block template's interaction.
- `is_valid_bonemeal_target` refuses bone meal. For `on_tree_grow`, the soil under the trunk stays
  unchanged and the rest of the tree grows normally.
- `dispense_method` retains the original input stack and emits no dispense event.
- `tool_modified_state` leaves the block unchanged. The global `block_tool_modification` hook still
  hears the tool's candidate state.
- Block entities belonging to the disabled mod do not tick, including through native tick loops.
  Their persistent state stays in the world and resumes when enabled.

These defaults occupy the disabled owner's usual place in Part dispatch: earlier enabled Parts
can answer first. Scheduled block ticks are held before dispatch where the disabled owner subscribed.
A disabled mod's Part attached to another owner's block does not make that block inert. Read-only queries
(such as shapes, fluid state and loot) retain their ordinary data/template fallback. This is the
same on native and wasm; enabled listeners and hook versions do not change.

Items and entities keep their registered data and default engine behaviour while their Part
callbacks are disabled. They are not globally frozen by this block-specific policy. In particular,
item use code is skipped but ordinary data-driven item behaviour remains; entity movement and
vanilla AI are not suspended. Do not switch a mod off as a way to pause its entities.
