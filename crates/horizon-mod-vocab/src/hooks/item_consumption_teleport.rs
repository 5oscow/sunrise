//! The `item_consumption_teleport` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0187

pub trait Hooks {
    /// On an authoritative server, a cauldron at `pos` is about to change
    /// fill level from `from_level` to `to_level` for `reason`. Set `cancel`
    /// to leave it unchanged.
    #[hook(
        signature = "pub(in crate::game) fn native_cauldron_level_changed( &self, call: mod_api::__private::CauldronLevelChangedCall<'_>, )",
        side = "server",
        contract = "cancel",
        out = "cancel",
        neoforge = "none (NeoForge has no such event; Bukkit's CauldronLevelChangeEvent)",
        cancellable = true,
        target = "horizon_client::game::native_hooks::base::GameLevel<'_>::native_cauldron_level_changed"
    )]
    fn cauldron_level_changed(
        dimension: LevelDimension,
        player: Option<u128>,
        pos: BlockPos,
        from_level: i32,
        to_level: i32,
        reason: &str,
        cancel: &mut bool,
    );
    /// A server player or fox that ate chorus fruit (or another item with the
    /// `teleport_randomly` consume effect) is about to try a random spot
    /// (NeoForge's `EntityTeleportEvent.ItemConsumption`, vanilla's
    /// `TeleportRandomlyConsumeEffect.apply`): once per attempt (up to 16),
    /// after they stopped riding. `from` is their feet, `target` the spot
    /// drawn; a mod may move it (its y kept inside the level's height, a
    /// value that is not a number ignored), and the game still looks down
    /// from it for ground it may land on. Canceling ends the effect: no teleport and no
    /// more attempts. `consumer` names the living actor through its handle.
    /// `consumed` is the stack captured before consumption, including
    /// its count and components; it is not reread from the hand after listener writes.
    /// A player's state is swapped in (`host.players().acting_player()`). A fox's
    /// callback has the full server host before its living AI/movement step and
    /// before the next mob ticks. Listener writes survive; changing the consumer's
    /// item, position or eligibility refuses the captured effect instead of
    /// overwriting that write. Cancellation still finishes consumption.
    /// Diverges from upstream: renamed from
    /// `chorus_fruit_teleport`.
    #[hook(
        signature = "pub(in crate::game) fn native_item_consumption_teleport( &mut self, consumer: mod_api::LivingRef, consumed: &mod_api::ItemStack, from: [f64; 3], target: &mut [f64; 3], cancel: &mut bool, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_item_consumption_teleport",
        side = "server",
        contract = "cancel",
        version = 2,
        out = "cancel",
        neoforge = "EntityTeleportEvent.ItemConsumption",
        cancellable = true
    )]
    fn item_consumption_teleport();
}
