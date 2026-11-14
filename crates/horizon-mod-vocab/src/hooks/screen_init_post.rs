//! The `screen_init_post` hooks and helpers: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// A screen was observed opening or changing rectangle, at most once per frame
    /// (`ScreenEvent.Init.Post`).
    #[hook(
        signature = "pub(in crate::game) fn native_screen_init_post(&self, screen: &ScreenView<'_>)",
        side = "client",
        contract = "listen",
        neoforge = "ScreenEvent.Init.Post",
        cancellable = false
    )]
    fn screen_init_post();
}
