//! The `player_recipe_book_settings_change` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0305

pub trait Hooks {
    /// A server player's authoritative recipe book pair changed. `book` is a
    /// typed RecipeBookIdentity, never a custom vanilla ordinal. Rejected and
    /// unchanged custom requests do not notify. The guest is not swapped in.
    #[hook(
        signature = "pub(in crate::game) fn native_player_recipe_book_settings_change( &mut self, player: u128, name: &str, book: mod_api::RecipeBookIdentity, open: bool, filtering: bool, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_player_recipe_book_settings_change",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event; Bukkit's PlayerRecipeBookSettingsChange)",
        paper = "PlayerRecipeBookSettingsChangeEvent"
    )]
    fn player_recipe_book_settings_change();
}
