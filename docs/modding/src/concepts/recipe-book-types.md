# Recipe book types and saved flags

Register a custom recipe book during shared mod registration. Its namespaced ID is
its stable identity for menus, per-player state, and negotiated synchronization.
Native and wasm mods use the same registrar and persistence rules.

`recipe_book_type("example:cooking")` saves the book's open and craftable-only
filtering flags in the engine's `sunrise:book_settings` compound inside the
player's `recipeBook` compound. Existing books using this declaration keep that
layout, including entries saved while their mod is absent.

For a book that needs Java-compatible flat settings, declare the two saved field
names instead:

```rust,ignore
let cooking = registrar.recipe_book_type_with_fields(
    "example:cooking",
    "isModCookingGuiOpen",
    "isModCookingFilteringCraftable",
)?;
```

This follows vanilla's `RecipeBookSettings.TypeSettings` codec: both names are
fields directly inside `recipeBook`, beside `isFurnaceGuiOpen` and the other
vanilla flags. A true flag is written as `1b`; a false flag is omitted. Missing
fields read as false. A player file from a Java world with those field names
therefore restores the same flags. The book is not also written to the engine
extension, and extension values are not a fallback for declared flat fields.
Changing a book's storage declaration is not a migration mechanism.

Names preserve their exact spelling and may contain any characters. Each must be
nonempty and at most 256 UTF-8 bytes. The two names must differ, and neither may
clash with any other book's declared field, vanilla's eight flag fields,
`recipes`, `toBeDisplayed`, or `sunrise:book_settings`. Invalid declarations refuse
installation even if the mod ignores the returned error. IDs still follow the
normal namespaced-ID ownership, 256-byte, and 1024-book limits. Registration
freezes before player files are loaded.

The saved field names do not change the book ID used by menus or synchronization.
Both flags live in the player's whole recipe-book state, rather than in a menu
instance. Custom panels wait for the server's negotiated settings snapshot before
using that state; local intent does not overwrite the authoritative flags.
