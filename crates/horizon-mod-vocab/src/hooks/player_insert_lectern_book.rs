//! The `player_insert_lectern_book` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0254

pub trait Hooks {
    /// A server player inserted a book into a lectern, after the block entity and inventory changed.
    /// `book` is the item id. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_player_insert_lectern_book( &mut self, player: u128, name: &str, pos: [i32; 3], book: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_player_insert_lectern_book",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn player_insert_lectern_book();
}
