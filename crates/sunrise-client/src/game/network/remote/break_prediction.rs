use super::*;

impl Game<'_> {
    /// MultiPlayerGameMode.destroyBlock: the predicted break.
    pub(super) fn predict_break(&mut self, pos: [i32; 3], state: BlockStateId, sequence: i32) {
        self.tutorial_destroy(state, 1.0);
        // Block.playerWillDestroy's destroy event (particles and the break
        // sound), which BaseFireBlock skips: the server's extinguish sound
        // is all a player hears.
        if !self.res.facts().get(state).has_class("BaseFireBlock") {
            self.play_break_sound(pos, state);
            self.destroy_particles(pos, state);
        }
        // Run the local shape updates over the removal, including the fluid left behind.
        // Keep every resulting write under the break's sequence, so packet reconciliation
        // can correct them without running the local rules again.
        let [x, y, z] = pos;
        let pos = sunrise_core::position::BlockPos::new(x, y, z);
        let changes = self.with_level(|b, base, ticks| {
            let mut level = DryLevel::new(base);
            level.reads = Some(ticks);
            b.remove_block(
                &mut level,
                &mut sunrise_world::level::Ticks::default(),
                pos,
                false,
            );
            level.changed.into_iter().collect()
        });
        self.predict_blocks(&changes, sequence);
        self.mining = None;
    }
}
