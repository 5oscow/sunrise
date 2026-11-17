//! The server liquid-container write.

pub trait Hooks {
    /// `LiquidBlockContainer.placeLiquid`, before the template fills `state` at
    /// `pos`. A bucket supplies a source `fluid`; spreading supplies the state
    /// that would flow there. The first `Some(bool)` replaces the template.
    /// Return true after storing the fluid and scheduling any tick it needs,
    /// or false when it was not placed. `None` leaves the template to run.
    /// A plain bucket still empties if the container returns false, unlike a
    /// validated fluid-container operation.
    ///
    /// Server only. Player buckets and the fluid utility journal the listener's
    /// writes with the bucket action. Spreading and dispensers use the direct
    /// level write host; during an open transaction they log and skip the
    /// listener, leaving the template's answer.
    #[hook(
        side = "server",
        contract = "replace",
        scope = "block",
        key = "state",
        out = "answer",
        target = "sunrise_client::game::native_hooks::place_liquid::GameLevel<'_>::native_place_liquid",
        neoforge = "none (vanilla LiquidBlockContainer.placeLiquid)"
    )]
    fn place_liquid();
}
