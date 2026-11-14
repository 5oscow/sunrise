//! The `exp_bottle` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0211

pub trait Hooks {
    /// An experience bottle broke and its experience was awarded.
    #[hook(
        signature = "pub(in crate::game) fn native_exp_bottle(&mut self, pos: [f64; 3], experience: u32)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_exp_bottle",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn exp_bottle();
}
