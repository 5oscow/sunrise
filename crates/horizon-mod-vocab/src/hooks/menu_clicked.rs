//! The `menu_clicked` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0034

pub trait Hooks {
    /// A click in a menu a mod opened (`native_open_menu`), which moved
    /// nothing: the menu's `slot` (its own first, then the player's
    /// inventory, `native_menu_inventory_slot`; -999 outside), the `button`
    /// and the click's `mode` (`ClickType`'s id) of the player `uuid`.
    /// No player's state is swapped in: the menu helpers take the player's
    /// UUID.
    #[hook(
        signature = "pub(in crate::game) fn native_menu_clicked( &mut self, uuid: u128, window: i32, slot: i32, button: i32, mode: i32, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_menu_clicked",
        side = "server",
        contract = "listen",
        neoforge = "none (vanilla AbstractContainerMenu.clicked; NeoForge adds no event or extension for it)"
    )]
    fn menu_clicked();
}
