# Set the item panel's search text

On the client, `panel_search(mod_id, text)` puts `text` in the search box of the mod's
`panel_items` panel, as if the player had typed it. It is a `WorldOnly` operation, like
`panel_style`.

- The box's value is replaced, cut to its 50 characters, with the cursor at the end.
- The mod hears the change as `panel_event` `Search(text)` the next time its panel draws, the
  same event typing sends, so its filter changes in one place. The event never fires during the
  call itself, so calling it from inside one of your own hooks is safe.
- Only the panel's owner can set its text: a mod that isn't showing the panel changes nothing.
  Setting the text the box already holds sends no event.
- Set while no container screen is open, the text stays in the box and is heard when the panel
  next appears.

```rust
// AE2's "Clear JEI search on open": the terminal empties the viewer's box.
host.panel_search("jei", "");
```

This is JEI's `IIngredientFilter.setFilterText`; the recipe book's own box does the same with
`searchBox.setValue`.
