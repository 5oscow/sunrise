//! The client's stored pick decision, at both vanilla pick call sites.

pub trait Hooks {
    /// Whenever vanilla picks: once per client tick at `partial_tick = 1.0`,
    /// after the game mode tick and before key handling (also while paused),
    /// and once per frame at the frame's game-time partial tick before extraction.
    /// Both sites overwrite the same stored result. `original` is the vanilla
    /// answer; block and entity candidates are supplied independently. `result`
    /// starts at `original`. Listeners run in dependency order and may replace
    /// or clear only `result`. Tick inputs, including both hands of use, read
    /// the tick's result without another pick. The highlight reads the frame's
    /// result, which can differ from the tick's. Reentrant crosshair_target
    /// reads see `original`. A replacement is validated once at writeback;
    /// invalid output becomes a miss, never a recomputed pick. Consumption does
    /// not recheck block state or entity liveness. Server checks and item-specific
    /// rays (including buckets and brushes) are unchanged. No NeoForge 26.3 counterpart.
    #[hook(
        side = "client",
        contract = "collect",
        out = "result",
        neoforge = "none (vanilla Minecraft.pick, before tick inputs and frame extraction)"
    )]
    fn client_pick(
        partial_tick: f32,
        original: Option<CrosshairTarget>,
        block_candidate: Option<CrosshairTarget>,
        entity_candidate: Option<CrosshairTarget>,
        result: &mut Option<CrosshairTarget>,
    );
}
