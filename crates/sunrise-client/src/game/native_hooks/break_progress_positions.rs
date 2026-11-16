//! One validated, stored display answer per crack stage, never per frame.
use super::*;
use crate::game::api_values::{TryIntoApi, block_state::block_listens};

impl Game<'_> {
    pub(in crate::game) fn with_break_progress_positions(
        &mut self,
        pos: [i32; 3],
        progress: u8,
        publish: impl FnOnce(&mut Self, Vec<[i32; 3]>),
    ) {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if !self.native_window() || !block_listens(mod_api::HookId::BreakProgressPositions, state) {
            publish(self, Vec::new());
            return;
        }
        let Some(level) = self.native_client_level() else {
            publish(self, Vec::new());
            return;
        };
        let Ok(state) = state.try_into_api() else {
            publish(self, Vec::new());
            return;
        };
        let seal = self.transaction_open();
        let mut positions = Vec::new();
        self.native_break_progress_positions(mod_api::__private::BreakProgressPositionsCall {
            level,
            pos: mod_api::BlockPos::new(pos[0], pos[1], pos[2]),
            state,
            progress,
            positions: &mut positions,
        });
        let mut extras = Vec::new();
        let mut refused = 0_usize;
        for candidate in positions {
            let candidate = [candidate.x, candidate.y, candidate.z];
            let delta: [i64; 3] =
                std::array::from_fn(|i| i64::from(candidate[i]) - i64::from(pos[i]));
            if candidate == pos
                || extras.contains(&candidate)
                || extras.len() == 64
                || !delta.iter().all(|d| d.abs() <= 8)
                || delta.iter().map(|d| d * d).sum::<i64>() > 64
            {
                refused += 1;
            } else {
                extras.push(candidate);
            }
        }
        if refused > 0 {
            log::debug!(
                "break_progress_positions refused {refused} extras outside the crack display bounds or duplicated positions"
            );
        }
        if self.native_client_level() == Some(level) {
            publish(self, extras);
        } else {
            log::debug!("break_progress_positions discarded extras after the client level changed");
        }
        self.transaction_close(&seal, true);
    }

    pub(in crate::game) fn native_break_progress_positions(
        &mut self,
        call: mod_api::__private::BreakProgressPositionsCall<'_>,
    ) {
        mod_api::dispatch_break_progress_positions(self, call);
    }
}
