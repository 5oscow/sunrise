//! Carrying: a moving collider (an entity's `set_collider`) moves the bodies that stand on it.
//!
//! Once a tick, after a level's entities ticked and so whatever order they ticked in, every
//! body standing on a collider box moves by the displacement its contact point had under that
//! collider's change this tick (translation and rotation), as a collided `Entity.move` with
//! `MoveCause::Carried`: the move a lid's push makes, through the same gate, so it stops at
//! walls, steps, and is seen by an `entity_move` listener. Yaw is never turned.
//!
//! A body's support is the one collider box under it: the highest top, a tie to the lower
//! collider entity id. The supports form a forest, and it is carried from the roots: a collider
//! that stands on another collider is carried first, and the bodies on it use its whole
//! displacement. A body that rides something is not carried; it follows its vehicle. The server
//! carries what it simulates, each client its own player.
use super::*;
use sunrise_client_render::moving_colliders::{self, Prepared};

/// How far under its feet a body may have a box and still stand on it.
const SUPPORT_PROBE: f64 = 0.01;

/// A body standing on a collider.
struct Rider {
    target: MoveWho,
    id: u32,
    /// The collider entity it stands on.
    support: u32,
}

impl Game<'_> {
    /// Carries the bodies standing on the colliders of the level that moved this tick.
    /// `start` is the colliders as the tick began, where the bodies stood.
    pub(in crate::game) fn carry_pass(&mut self, start: &Prepared) {
        if start.is_empty() {
            return;
        }
        let riders = self.collider_riders(start);
        if riders.is_empty() {
            return;
        }
        let ids: Vec<u32> = start.placed().map(|p| p.id).collect();
        let mut done: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut remaining = ids.clone();
        let mut stand_grid = None;
        while !remaining.is_empty() {
            // A collider is ready once the collider its own entity stands on is done.
            let (mut ready, rest): (Vec<u32>, Vec<u32>) = remaining.iter().partition(|collider| {
                riders
                    .iter()
                    .find(|r| r.id == **collider)
                    .is_none_or(|r| done.contains(&r.support) || !ids.contains(&r.support))
            });
            let mut rest = rest;
            if ready.is_empty() {
                // A cycle of colliders on one another: carried in the order they were placed.
                ready.append(&mut rest);
            }
            // The carried moves meet the colliders where they stand now.
            let now = self.placed_moving_colliders();
            moving_colliders::in_level(now, || {
                for collider in &ready {
                    self.carry_riders_of(*collider, start, &riders, &mut stand_grid);
                }
            });
            done.extend(ready);
            remaining = rest;
        }
    }

    /// The bodies standing on a collider of `start`, each with the collider it stands on.
    fn collider_riders(&mut self, start: &Prepared) -> Vec<Rider> {
        let server = self.is_logical_server();
        let steps = self.push_step_heights();
        let mut riders = Vec::new();
        for target in self.push_targets() {
            let wanted = if server {
                matches!(target, MoveWho::Entity(_))
            } else {
                matches!(target, MoveWho::Player)
            };
            if !wanted {
                continue;
            }
            let Some(body) = self.push_body(target, None, &steps) else {
                continue;
            };
            if !body.on_ground {
                continue;
            }
            let (id, no_physics, rides) = self.carry_facts(target);
            if no_physics || rides {
                continue;
            }
            let mut best: Option<(f64, u32)> = None;
            for placed in start.placed() {
                if placed.exempt.contains(&id) {
                    continue;
                }
                let allowed = placed
                    .collider
                    .collide(body.bounds, Axis::Y, -SUPPORT_PROBE);
                if allowed <= -SUPPORT_PROBE {
                    continue;
                }
                let gap = -allowed;
                let better = best
                    .is_none_or(|(g, c)| gap < g - 1.0e-9 || (gap <= g + 1.0e-9 && placed.id < c));
                if better {
                    best = Some((gap, placed.id));
                }
            }
            if let Some((_, support)) = best {
                riders.push(Rider {
                    target,
                    id,
                    support,
                });
            }
        }
        riders
    }

    /// The entity id of a carried body, whether it has no physics, and whether it rides.
    fn carry_facts(&self, target: MoveWho) -> (u32, bool, bool) {
        match target {
            MoveWho::Player => (
                self.player.as_ref().map_or(0, |p| p.entity_id as u32),
                false,
                self.riding.is_some(),
            ),
            MoveWho::Guest(_) => (0, true, false),
            MoveWho::Entity(handle) => {
                let (id, no_physics) = with_entity!(self, handle, entity => {
                    let stored = self.persist.no_physics.get(&entity.id()).copied();
                    (entity.id(), entity.no_physics(stored))
                });
                (id, no_physics, self.push_root(target) != Some(id))
            }
        }
    }

    /// Moves each body standing on `collider` by the displacement of its contact point.
    fn carry_riders_of(
        &mut self,
        collider: u32,
        start: &Prepared,
        riders: &[Rider],
        stand_grid: &mut Option<sunrise_client::entities::ObstacleGrid>,
    ) {
        let Some(before) = start.placed().find(|p| p.id == collider) else {
            return;
        };
        let Some(after) = self.collider_now_frame(collider) else {
            return;
        };
        let steps = self.push_step_heights();
        for rider in riders.iter().filter(|r| r.support == collider) {
            let Some(body) = self.push_body(rider.target, None, &steps) else {
                continue;
            };
            let centre = body.bounds.center();
            let contact = Vec3::new(centre.x, body.bounds.min(Axis::Y), centre.z);
            let carried = after
                .to_world(before.collider.frame.to_local(contact))
                .subtract(contact);
            if carried.length_squared() < 1.0e-18 {
                continue;
            }
            self.move_by_lid(Lid::Carried, rider.target, body, carried, false, stand_grid);
            // It still stands on what carries it: a sideways or rising carry must not leave it
            // airborne for the next tick to find.
            match rider.target {
                MoveWho::Player => {
                    if let Some(player) = self.player.as_mut() {
                        player.ground.on_ground = true;
                    }
                }
                MoveWho::Entity(handle) => {
                    with_entity_mut!(self, handle, entity => entity.set_piston_on_ground(true));
                }
                MoveWho::Guest(_) => {}
            }
        }
    }
}
