//! The `client_services` declarations reconciled with the fork API.

pub trait Hooks {
    /// Before an actual connection is discarded, not on a dimension change.
    #[hook(
        signature = "pub(in crate::game) fn native_client_disconnecting(&self)",
        side = "client",
        contract = "listen",
        neoforge = "ClientPlayerNetworkEvent.LoggingOut"
    )]
    fn client_disconnecting();
    /// The normal menu blur request, before finite/range validation. None disables it.
    #[hook(
        signature = "pub(in crate::game) fn native_menu_background(&self, radius: &mut Option<f32>)",
        side = "client",
        contract = "listen",
        neoforge = "none (NeoForge has no menu blur event)"
    )]
    fn menu_background();
}
