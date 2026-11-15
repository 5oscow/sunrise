# Screens, the inventory key and the sign editor

A client mod can close the screen a player has open, open the player's inventory, and read which sign
the sign editor is on. The two writes take the same paths as the keys the player uses, so the server
sees only the packets of a normal client.

```rust
// Escape: close whatever is open.
if host.close_screen() {
    // a screen was open and is now closed
}

// The inventory key, key.inventory.
if host.open_inventory() {
    // the inventory screen is up (or the ridden vehicle's screen was asked for)
}

// Which sign the editor is on, if any.
if let Some(sign) = host.sign_editor() {
    let _ = (sign.pos, sign.front);
}
```

`ClientHost::close_screen()` follows the screen's own Escape close. A container screen closes as the
player's container close: the client sends `ServerboundContainerClosePacket` with the menu's id and then
closes its copy of the menu. It never drops a container menu locally alone, which would leave the
server's menu open. A pause menu closes to the game, another screen runs its own close, and the sign
editor sends `ServerboundSignUpdatePacket` with the lines being edited, as Escape does. The death
screen does not close this way. The screen hooks' closing notices fire as they do for Escape. It returns
`false` when no screen is open.

`ClientHost::open_inventory()` does what `key.inventory` does: it opens the player's inventory screen
locally and sends nothing, because the player's menu is always open on the server. Riding a vehicle
with its own inventory (a tamed mount or a chest boat), it sends the open-inventory player command
(`ServerboundPlayerCommandPacket`) at the player's next tick instead, as the key's press is. It returns
`false` with a screen already open, for a spectator, in a portal transition, or with no world.

`ClientHost::sign_editor()` returns `Some(SignEditView { pos, front })` while the sign editor is open:
the sign's block position and whether its front side is being edited. It returns `None` with no sign
editor open, or outside a world. A close of the sign editor always sends the sign update, however it
closes; there is no close that skips it, because the vanilla client never omits that packet.

## Moving a mod screen's slots

A mod screen declares where each slot sits in `Screen::slots`. A screen whose layout changes while it is
open (tabs, a collapsing panel, a scrolling slot grid) moves a slot afterwards with
`ClientHost::set_screen_slot_position`:

```rust,ignore
// A scrolling grid: slot 3 now sits one row up, 18 pixels from the screen's top-left corner.
if !host.set_screen_slot_position(3, 62.0, 18.0) {
    // no mod screen is open, the screen does not show slot 3, or the position is not finite
}
```

`slot` is the index into `Screen::slots`. `x` and `y` are GUI pixels from the screen's top-left corner and
name the item's top-left corner, as in `Screen::slots`. The call takes `&self`, so a screen hook such as
`screen_render_post` can move slots every frame; a call that repeats a slot's position changes nothing and costs
no layout work. The position lasts until the screen closes, and the next screen opens with its declared layout.

From the next frame the moved slot is a vanilla slot at that position, which is what the screen reads live:

- the hovered slot is the first shown slot in index order whose 18 by 18 box (one pixel up and left of the item
  square, one beyond it) holds the pointer, so slots that overlap resolve to the lower index;
- the 24 by 24 highlight is drawn 4 pixels up and left of it;
- its item, count and decorations, and its empty-slot icon, are drawn there;
- the recipe book's ghost items are drawn there.

The recipe book and `screen_room` still move the whole image, and every slot with it; a slot's own position
never changes for them. A position outside the image is allowed and is not clamped. The call is client-side
only: nothing is sent, and the server's menu never learns a position.

It refuses, returning `false` and changing nothing, with no mod screen open, for an index past the end of
`Screen::slots` or hidden there with `None`, and for an `x` or `y` that is NaN or infinite. It never panics.
It moves only the slots a mod screen declares; the player's inventory stays under `inventory_x` and
`inventory_y`, and showing or hiding a slot stays with `Screen::slots`.

## Labels: wrapped text without a widget

A label is text laid out in a box: measured, wrapped and placed, and nothing else. Call
`DrawContext::text_lines` from a drawing hook with the component, the box's width, an alignment and the
spacing between lines, and draw the lines it answers with where it says:

```rust
// From screen_render_post, gui_render_post or another drawing hook.
let lines = draw.text_lines(&label, 120.0, TextAlign::Centre, 2.0)?;
let mut draws = Vec::with_capacity(lines.lines.len());
for line in &lines.lines {
    draws.push(GuiDraw::Text {
        text: line.text.clone(),
        x: (x + line.at[0]) as i32,
        y: (y + line.at[1]) as i32,
        color: 0xFFFF_FFFF,
        shadow: false,
    });
}
draw.gui_paint(&draws)?;
// lines.width and lines.height size the box you reserved for it.
```

Wrapping is vanilla's: a line breaks at a space that fits, a single word wider than the box is broken at
a character rather than dropped, and an explicit `\n` starts a new line. A width of zero or less, or one
that is not finite, means no wrapping at all. Each line keeps its styles, so `GuiDraw::Text` is what draws
it; `DrawContext::text` draws plain characters only. `height` is the last line's `at[1]` plus one line
height, so a scroll container sizes its content from it, and an empty component has no line at all. Text
deeper than 512 component nodes is refused, as the other text-drawing methods refuse it.
