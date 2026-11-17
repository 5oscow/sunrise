//! The `screen` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0027

pub trait Hooks {
    /// A menu of mod menu type `menu` opens on this client: set `screen` to
    /// lay its screen out (`screen_draw` draws it), whatever opened the menu
    /// (a block, an item's use, an entity, a command). `block` is the mod
    /// block at the menu's position whose container opens this menu type,
    /// and `None` when there is none (`player_open_mod_menu` without such a
    /// position): blocks that share a menu type branch on it. A mod menu type
    /// with no answer has no screen, and its window closes again. For a
    /// block's menu the server asks it too: a slot it hides is not in the
    /// menu there either, so shift-clicks never fill it. A menu opened with
    /// `player_open_mod_menu` keeps every slot on the server, as a
    /// `set_screen_slot` hide does. A recipe-book menu's registered screen opens only for its
    /// block's menu.
    #[hook(
        version = 2,
        side = "both",
        contract = "replace",
        out = "screen",
        scope = "menu",
        key = "menu",
        host = "read",
        neoforge = "RegisterMenuScreensEvent.register (one screen per menu type, whatever opened the menu; Sunrise's is a layout value and three drawing and input hooks, not a Screen class)"
    )]
    fn screen();
}
