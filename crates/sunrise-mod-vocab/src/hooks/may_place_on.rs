//! The `may_place_on` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0079

pub trait Hooks {
    /// `VegetationBlock.mayPlaceOn` for the plant `state` on `ground` at
    /// `ground_pos`, before the plant answers it: the blocks a mod plant (one
    /// whose template is a vanilla plant, such as a crop) can stand on. Its
    /// template's `canSurvive` asks it, so the plant is refused there and
    /// pops off when its ground changes to one it can't stand on. Answer only
    /// for your own blocks. The level is read-only.
    /// Asked on both sides, as vanilla asks it on both: a client asks it from its placement
    /// prediction and its own shape updates, of its loaded copy of the level (match `LevelSide`; a
    /// read the client can't answer is `None`). Answer `None` on a client where only a server can
    /// decide: the template's rule then predicts, and the server still decides. Named injections
    /// request `hook_version = 2`.
    #[hook(
        side = "both",
        version = 2,
        contract = "replace",
        out = "may",
        scope = "block",
        key = "state",
        target = "sunrise_client::game::native_hooks::base::GameLevel<'_>::native_may_place_on",
        neoforge = "IBlockExtension.canSustainPlant"
    )]
    fn may_place_on();
}
