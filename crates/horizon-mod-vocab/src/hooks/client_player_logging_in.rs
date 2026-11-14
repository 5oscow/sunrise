//! The `client_player_logging_in` hooks and helpers: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// The client accepted a play login and initialized its level and player
    /// identity (NeoForge's `ClientPlayerNetworkEvent.LoggingIn`). `player_id`
    /// is the server's entity ID. Client-side,
    /// once per connection, including single player.
    #[hook(
        signature = "pub(in crate::game) fn native_client_player_logging_in(&mut self, player_id: i32, uuid: u128)",
        side = "client",
        contract = "listen",
        neoforge = "ClientPlayerNetworkEvent.LoggingIn",
        cancellable = false
    )]
    fn client_player_logging_in();
}
