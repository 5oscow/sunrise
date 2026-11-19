//! The block part for vanilla's `Block.setPlacedBy`.

pub trait Hooks {
    /// Runs on the server once for the placed block, after vanilla's `Block.setPlacedBy`
    /// work and before `ItemPart::on_block_placed_by` for a block-item placement.
    /// `state` identifies the block that ran the callback, even if vanilla's effects
    /// changed the cell. `stack` is the original, read-only stack before consumption,
    /// or `None` if no stack was supplied. `placer` is `None` for a dispenser or for
    /// code placement. `LevelRef::set_placed_by` also reaches this part, without
    /// firing the item notice. Writes through the `ServerHost` participate in the
    /// current transaction, if one is open.
    #[hook(
        side = "server",
        contract = "listen",
        scope = "block",
        key = "state",
        neoforge = "none (vanilla Block.setPlacedBy)"
    )]
    fn set_placed_by();
}
