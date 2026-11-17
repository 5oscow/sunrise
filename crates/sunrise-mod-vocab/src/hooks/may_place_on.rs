//! The `may_place_on` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0079

pub trait Hooks {
    /// On an authoritative server, `VegetationBlock.mayPlaceOn` for plant
    /// `state` on `ground` at `ground_pos`. Implementing this Part method
    /// declares vegetation behaviour independently of the block's template:
    /// placement and neighbour updates check the soil's `can_sustain_plant`
    /// first, then this method. `None` uses an inherited vegetation rule, or
    /// `minecraft:supports_vegetation` for other templates. Unsupported plants
    /// are refused and pop off. The level is read-only.
    #[hook(
        side = "server",
        contract = "replace",
        out = "may",
        scope = "block",
        key = "state",
        target = "sunrise_client::game::native_hooks::base::GameLevel<'_>::native_may_place_on",
        neoforge = "IBlockExtension.canSustainPlant"
    )]
    fn may_place_on();
}
