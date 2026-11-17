//! The `can_survive` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0078

pub trait Hooks {
    /// `BlockState.canSurvive` for `state` at `pos`
    /// (`BlockBehaviour.canSurvive`), before the block answers it: as it is
    /// placed (`BlockItem.canPlace`), from a shape update of a block whose
    /// template checks it (a plant, a torch, a sign: false breaks the block
    /// with its drops), and wherever else vanilla asks. `state` may not be in
    /// the world yet, so decide from `state`. Answer only for your own
    /// blocks: `Some(false)` refuses the spot, `Some(true)` allows it, and
    /// the block's own rule doesn't run. The level is read-only. Asked on
    /// every shape update of such a block: keep it short.
    /// Asked on both sides, as vanilla asks it on both: a client asks it from its placement
    /// prediction and its own shape updates, of its loaded copy of the level (match `LevelSide`; a
    /// read the client can't answer is `None`). Answer `None` on a client where only a server can
    /// decide: the template's rule then predicts, and the server still decides. Named injections
    /// request `hook_version = 2`.
    #[hook(
        side = "both",
        version = 2,
        contract = "replace",
        out = "survives",
        scope = "block",
        key = "state",
        target = "sunrise_client::game::native_hooks::base::LevelView<'_>::native_can_survive",
        neoforge = "none (vanilla BlockBehaviour.canSurvive; NeoForge adds no event or extension for it)"
    )]
    fn can_survive();
}
