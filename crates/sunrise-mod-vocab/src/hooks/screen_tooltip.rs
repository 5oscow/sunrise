//! The `screen_tooltip` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0030

pub trait Hooks {
    /// The tooltip at `at` (GUI pixels from the top left) of the screen of
    /// mod menu type `menu` (`block` as in `screen`).
    /// `slot` is the hovered menu index, including player inventory slots,
    /// or `None` outside a mapped slot. On a filled slot, `lines` starts as
    /// the item's tooltip after item tooltip hooks; your final lines replace
    /// it, as `AbstractContainerScreen.getTooltipFromContainerItem` does.
    /// Empty slots and screen space start empty. Images, style and the carried
    /// stack visibility rule are unchanged. Lines are rich Text/color pairs,
    /// so editing one line does not flatten the styling of the others.
    #[hook(
        version = 3,
        side = "client",
        contract = "collect",
        out = "lines",
        scope = "menu",
        key = "menu",
        neoforge = "none (see screen)"
    )]
    fn screen_tooltip();
}
