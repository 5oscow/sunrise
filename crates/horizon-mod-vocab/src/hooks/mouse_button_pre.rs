//! The `mouse_button_pre` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0116

pub trait Hooks {
    /// Raw mouse button input on any screen, before dispatch; cancel skips dispatch and Post.
    /// Button/modifiers use the same GLFW encoding as screen hooks. Synthetic clicks skip it.
    #[hook(
        signature = "pub(in crate::game) fn native_mouse_button_pre( &mut self, button: i32, pressed: bool, modifiers: u32, canceled: &mut bool, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_mouse_button_pre",
        side = "client",
        contract = "cancel",
        out = "canceled",
        neoforge = "InputEvent.MouseButton.Pre",
        cancellable = true
    )]
    fn mouse_button_pre();
}
