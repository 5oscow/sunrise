//! The `may_place_on` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0079

pub trait Hooks {
    /// `VegetationBlock.mayPlaceOn` for the plant `state` on `ground` at
    /// `ground_pos`, before the plant answers it: the blocks a mod plant (one
    /// whose template is a vanilla plant, such as a crop) can stand on. Its
    /// template's `canSurvive` asks it, so the plant is refused there and
    /// pops off when its ground changes to one it can't stand on. Answer only
    /// for your own blocks. The level is read-only.
    /// Asked on both sides, as vanilla asks it: a client asks it from its
    /// placement prediction and its own shape updates, with its loaded copy of
    /// the level (match `LevelSide`). Reads a client can't answer, tag tests
    /// among them, are `None` there, and `BlockEntityPartState::state_in` gives
    /// its default: never answer from those as if they were the server's.
    /// Answer `None` on a client where only a server can decide: the template's
    /// rule then predicts, and the server still decides. Named injections
    /// request `hook_version = 2`.
    #[hook(
        side = "both",
        version = 2,
        contract = "replace",
        out = "may",
        scope = "block",
        key = "state",
        target = "sunrise_client::game::native_hooks::base::LevelView<'_>::native_may_place_on",
        neoforge = "IBlockExtension.canSustainPlant"
    )]
    fn may_place_on();
}
