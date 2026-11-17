//! The `pre_remove_side_effects` hooks and helpers: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// `BlockEntity.preRemoveSideEffects` of a mod's block entity: an
    /// authoritative server is removing `entity` from `pos`, where the block
    /// `state` stood, and its updates may have side effects (not under flag
    /// 256, `UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS`). Put in `drops` the stacks
    /// it held that the game doesn't: they scatter as `Containers.dropItemStack`
    /// scatters them, after the block entity's own container. The block entity
    /// is still in place, so its attachments read as they were. Answered from
    /// inside the level's write: don't change the world here.
    /// Set `keep_contents` to skip the base container slots. Each `drops` entry
    /// is a slot: `None` draws the empty slot's scatter position but spawns nothing.
    /// Then `spawns` creates each (stack, position) with default pickup delay,
    /// and `experience` awards each (amount, position) as experience orbs.
    /// Each output list accepts at most 1024 entries; invalid positions or stacks
    /// refuse the outputs. Positions must be finite. Default outputs draw nothing.
    #[hook(
        side = "server",
        contract = "collect",
        out = "drops",
        version = 2,
        scope = "block_entity",
        key = "entity",
        target = "sunrise_client::game::native_hooks::pre_remove_side_effects::GameLevel<'_>::native_pre_remove_side_effects",
        neoforge = "none (vanilla BlockEntity.preRemoveSideEffects; NeoForge adds no event or extension for it)"
    )]
    fn pre_remove_side_effects(
        pos: BlockPos,
        state: BlockState,
        entity: &ModBlockEntity,
        keep_contents: &mut bool,
        drops: &mut Vec<Option<ItemStack>>,
        spawns: &mut Vec<(ItemStack, [f64; 3])>,
        experience: &mut Vec<(u32, [f64; 3])>,
    );
}
