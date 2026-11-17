//! The forced respawn column's block-space predicate.

pub trait Hooks {
    /// Whether a player can respawn inside `state` at a forced spawn point that
    /// is neither a bed nor a respawn anchor (`Block.isPossibleToRespawnInThis`).
    /// Asked for the saved point and the block above it, on the server.
    /// `Some(true)` accepts the space, `Some(false)` refuses it, and `None`
    /// keeps the block's vanilla predicate. The state alone determines the
    /// answer; the host is read-only. This does not choose a bed or anchor's
    /// stand-up position (see `respawn_position`).
    #[hook(
        side = "server",
        contract = "replace",
        scope = "block",
        key = "state",
        out = "answer",
        host = "read",
        neoforge = "none (vanilla Block.isPossibleToRespawnInThis)"
    )]
    fn is_possible_to_respawn_in();
}
