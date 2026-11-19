//! The `can_place_item` hook: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// `Container.canPlaceItem`: whether `slot` of this mod block entity's container admits
    /// the offered nonempty `stack`. `Some(true)` or `Some(false)` replaces the declared
    /// `ModContainer::insert_rules` answer; `None` keeps it. Face, input-slot and stack
    /// limits still apply.
    ///
    /// This is a read-only query on the entity's saved data and current slots, with no Host.
    /// The engine may ask more than once and may never write to a queried slot. Seed a filter
    /// from the accepted write, not from this query. Hoppers, droppers, the built-in `ITEMS`
    /// handler's insert and `/loot replace block` ask it. Menus use `menu_slot_rules` instead
    /// (`Slot.mayPlace`). A subscribed wasm part costs one Wire call per queried slot.
    #[hook(
        side = "server",
        contract = "replace",
        out = "allowed",
        scope = "block_entity",
        key = "entity",
        neoforge = "none (vanilla Container.canPlaceItem; the item capability wraps this decision)"
    )]
    fn can_place_item();
}
