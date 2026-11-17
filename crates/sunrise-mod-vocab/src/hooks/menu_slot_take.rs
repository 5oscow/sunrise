//! Authoritative takes from registered mod menus.
pub trait Hooks {
    /// A player took `stack` from a mod-owned slot of `menu` (vanilla `Slot.onTake`
    /// and `Slot.onQuickCraft`; NeoForge counterpart: none). Server-only, after the
    /// source removal and at the take boundary, before the click continues. `slot`
    /// is the backing index in `container`, as in `menu_slot_rules`. `context`
    /// identifies the acting player and optional block, and snapshots the menu's
    /// containers at this boundary. `stack.count()` is the positive transferred
    /// count, not the source remainder. A quick move supplies that same count in
    /// `quick_craft_amount`; every other take supplies zero. A partial move is
    /// reported once, not once for each vanilla callback.
    ///
    /// Use the server Host to grant experience, unlock recipes, award crafted
    /// statistics or emit effects. This notice does not itself grant rewards,
    /// trigger the `recipe_crafted` criterion or run `onCraftedBy` item
    /// post-processing. It cannot change the delivered stack. The built-in
    /// crafting-grid result uses `item_crafted`, not this hook. Player inventory
    /// slots and vanilla menu types do not fire it. Client prediction transfers
    /// items without firing the notice; authoritative slots and effects follow.
    ///
    /// The listener and the remaining click share a transaction. A failed
    /// listener or changed continuation precondition refuses the click and
    /// rolls back its writes. Input/output changes in the mod's container are
    /// visible to the continuation, including a refilled result before a repeat.
    #[hook(
        side = "server",
        contract = "listen",
        scope = "menu",
        key = "menu",
        cancellable = false,
        version = 1,
        neoforge = "none (vanilla Slot.onTake and Slot.onQuickCraft)"
    )]
    fn menu_slot_take(
        menu: &str,
        slot: usize,
        container: MenuContainer,
        stack: &ItemStack,
        quick_craft_amount: u32,
        context: &MenuSlotContext,
    );
}
