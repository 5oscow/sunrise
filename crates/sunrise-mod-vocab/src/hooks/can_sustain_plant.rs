//! The `can_sustain_plant` hook: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// `IBlockExtension.canSustainPlant` for the soil `ground` at `ground_pos` and the plant
    /// `state` above it, asked before the plant answers its own `mayPlaceOn` (`may_place_on`)
    /// wherever vanilla's plant rule reads its ground: when the plant is placed, from its shape
    /// update and wherever else vanilla asks. `Some(true)` lets the plant stand there,
    /// `Some(false)` refuses it (a planted one pops off when its ground changes to this block), and
    /// left `None` the plant's own `may_place_on` answers, as vanilla's does. Answer only for your
    /// own soil. The level is read-only. Runs on every shape update of a plant over a part's block:
    /// keep it short.
    /// Asked on both sides, as vanilla asks it on both: a client asks it from its placement
    /// prediction and its own shape updates, of its loaded copy of the level (match `LevelSide`; a
    /// read the client can't answer is `None`). Answer `None` on a client where only a server can
    /// decide: the template's rule then predicts, and the server still decides. Named injections
    /// request `hook_version = 2`.
    #[hook(
        side = "both",
        version = 2,
        contract = "replace",
        out = "sustains",
        scope = "block",
        key = "ground",
        target = "sunrise_client::game::native_hooks::can_sustain_plant::GameLevel<'_>::native_can_sustain_plant",
        neoforge = "IBlockExtension.canSustainPlant"
    )]
    fn can_sustain_plant();
}
