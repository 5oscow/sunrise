//! One frame decision, shared by outline extraction and the next frame's inputs.
use super::*;
use crate::game::{Dim, pick_view_vector};
use mod_api::CrosshairTarget;

#[cfg(test)]
#[path = "client_frame_pick_tests.rs"]
mod tests;

#[derive(Default)]
pub(in crate::game) struct FramePick {
    pub ray: Option<([f64; 3], [f64; 3])>,
    epoch: Option<mod_api::WorldEpoch>,
    dimension: Option<Dim>,
    origin: [f64; 3],
    result: Option<CrosshairTarget>,
    dispatching: bool,
}

impl Game<'_> {
    pub(in crate::game) fn uses_frame_pick(&self) -> bool {
        self.native_window() && self.frame_pick.ray.is_none()
    }

    pub(in crate::game) fn frame_pick_distance(&self, point: [f64; 3]) -> f64 {
        (0..3)
            .map(|i| (point[i] - self.frame_pick.origin[i]).powi(2))
            .sum::<f64>()
            .sqrt()
    }

    /// A window always has a decision, including before the first pick (a miss).
    /// None in the outer option means a server ray or the frame's raw computation.
    pub(in crate::game) fn cached_entity_pick(&self) -> Option<Option<(EntityRef, f64, [f64; 3])>> {
        self.uses_frame_pick()
            .then(|| match self.crosshair_target() {
                Some(CrosshairTarget::Entity { entity, point }) => {
                    Some((entity, self.frame_pick_distance(point), point))
                }
                _ => None,
            })
    }

    /// Consumption checks identity/state, never geometry again and never a fallback ray.
    pub(in crate::game) fn stored_frame_pick(&self) -> Option<CrosshairTarget> {
        if self.frame_pick.dispatching {
            return self.frame_pick.result;
        }
        (self.frame_pick.epoch == self.native_lifecycle.epoch()
            && self.frame_pick.dimension == Some(self.dim))
        .then_some(())?;
        let result = self.frame_pick.result?;
        self.frame_pick_live(result).then_some(result)
    }

    fn frame_pick_live(&self, result: CrosshairTarget) -> bool {
        use crate::game::api_values::TryIntoApi;
        match result {
            CrosshairTarget::Block { pos, state, .. } => {
                self.world.block(pos.x, pos.y, pos.z).try_into_api().ok() == Some(state)
            }
            CrosshairTarget::Entity { entity, .. } => {
                self.native_client_entity_target(entity).is_some()
            }
            _ => false,
        }
    }

    /// The sole mod-output geometry boundary. Occlusion is intentionally not reapplied.
    fn accept_frame_pick(&self, result: CrosshairTarget) -> bool {
        let (point, reach) = match result {
            CrosshairTarget::Block { hit, .. } => {
                if !matches!(
                    hit.face,
                    [1, 0, 0] | [-1, 0, 0] | [0, 1, 0] | [0, -1, 0] | [0, 0, 1] | [0, 0, -1]
                ) {
                    return false;
                }
                (hit.point, self.block_reach)
            }
            CrosshairTarget::Entity { point, .. } => (point, self.frame_entity_pick_reach()),
            _ => return false,
        };
        point.iter().all(|v| v.is_finite())
            && self.frame_pick_distance(point) <= reach
            && self.frame_pick_live(result)
    }

    pub(in crate::game) fn refresh_frame_pick(&mut self, partial_tick: f32) {
        if !self.native_window() || self.title.is_some() || self.player.is_none() {
            return;
        }
        let Some(camera) = self
            .spectated_camera_snapshot()
            .or_else(|| self.player_camera_entity())
        else {
            return;
        };
        let origin = std::array::from_fn(|i| {
            camera.previous[i]
                + f64::from(partial_tick) * (camera.position[i] - camera.previous[i])
                + if i == 1 {
                    f64::from(camera.eye_height)
                } else {
                    0.0
                }
        });
        let direction = pick_view_vector(
            camera.view_y_rot(partial_tick),
            camera.view_x_rot(partial_tick),
        );
        self.frame_pick.ray = Some((origin, direction));
        self.frame_pick.origin = origin;
        self.frame_pick.epoch = self.native_lifecycle.epoch();
        self.frame_pick.dimension = Some(self.dim);
        let block_candidate = self.pick().and_then(|hit| {
            use crate::game::api_values::TryIntoApi;
            Some(CrosshairTarget::Block {
                pos: mod_api::BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]),
                state: self
                    .world
                    .block(hit.pos[0], hit.pos[1], hit.pos[2])
                    .try_into_api()
                    .ok()?,
                hit: hit.native(),
            })
        });
        let attack_candidate = self.frame_attack_range_entity();
        let entity_candidate = attack_candidate.or_else(|| self.frame_entity_candidate());
        let occluder = self
            .pick_unfiltered()
            .map(|hit| self.frame_pick_distance(hit.point));
        let original = attack_candidate.or_else(|| match entity_candidate {
            Some(CrosshairTarget::Entity { point, .. })
                if occluder.is_none_or(|distance| self.frame_pick_distance(point) < distance) =>
            {
                entity_candidate
            }
            _ => block_candidate,
        });
        self.frame_pick.result = original;
        self.frame_pick.ray = None;
        if !mod_api::__private::listened(mod_api::HookId::ClientFramePick) {
            return;
        }
        let seal = self.transaction_open();
        self.frame_pick.dispatching = true;
        let mut result = original;
        self.native_client_frame_pick(mod_api::__private::ClientFramePickCall {
            partial_tick,
            original,
            block_candidate,
            entity_candidate,
            result: &mut result,
        });
        self.frame_pick.dispatching = false;
        let valid_context = self.frame_pick.epoch == self.native_lifecycle.epoch()
            && self.frame_pick.dimension == Some(self.dim);
        self.frame_pick.result =
            result.filter(|&result| valid_context && self.accept_frame_pick(result));
        self.transaction_close(&seal, true);
    }

    pub(in crate::game) fn native_client_frame_pick(
        &mut self,
        call: mod_api::__private::ClientFramePickCall<'_>,
    ) {
        self.fire_client_frame_pick(call);
    }
}
