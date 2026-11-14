//! The `vault_loot` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0340

pub trait Hooks {
    /// A vault at `pos` is about to eject a loot item.
    #[hook(
        signature = "pub(in crate::game) fn native_vault_loot(&mut self, pos: [i32; 3], item: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_vault_loot",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event; Bukkit's BlockDispenseLoot)"
    )]
    fn vault_loot();
}
