//! The client's one stored pick decision per frame.

pub trait Hooks {
    /// Once per client frame, after its ticks and at its game-time `partial_tick`,
    /// before render-state extraction. `original` is the vanilla answer; the block
    /// and entity candidates are supplied independently. `result` starts at `original`.
    /// Listeners run in dependency order and may replace or clear only `result`.
    /// The highlight reads the committed result this frame; attack, use and pick-item
    /// read it in the next frame's ticks, without casting again. A reentrant
    /// crosshair_target read sees `original`. Invalid or stale output becomes a miss,
    /// never a recomputed pick. Server validation and item-specific rays (including
    /// buckets and brushes) are unchanged. NeoForge 26.3 has no counterpart.
    #[hook(
        side = "client",
        contract = "collect",
        out = "result",
        neoforge = "none (vanilla Minecraft.pick, once per frame before extraction)"
    )]
    fn client_frame_pick(
        partial_tick: f32,
        original: Option<CrosshairTarget>,
        block_candidate: Option<CrosshairTarget>,
        entity_candidate: Option<CrosshairTarget>,
        result: &mut Option<CrosshairTarget>,
    );
}
