//! The move check of a player a moving collider carries (docs/modding/src/how-to/native-mods.md,
//! "Moving colliders").
//!
//! A client moves its player with the collider as it last heard of it, so the position it
//! reports is in a frame that is some ticks old. The server keeps each collider's recent
//! transforms (`moving_colliders::History`) and judges a report in the world as the client saw
//! it: the collider placed at the age the report fits (`sunrise_core::geometry::carried`), the
//! move from the last accepted position, held in the collider's frame, run through the same
//! check a walking player gets. A report that fits no age in the window is corrected to where
//! the server last had the player on the collider, placed under the collider's transform now.
//! A correction is not a reason to count the player as floating or to kick it, and is counted.
use super::{Hub, TickCtx};
use std::sync::atomic::Ordering;
use sunrise_client_render::moving_colliders::{at_age, with_history};
use sunrise_core::geometry::{
    Aabb, Vec3,
    carried::{Ages, Body, Carried, Fit, Footing, judge},
};

/// How a report stands to the colliders of the level.
enum Plan {
    /// It fits a collider at an age.
    Fits {
        collider: u32,
        attached: bool,
        fit: Fit,
    },
    /// The player was on a collider and the report is on none: it has left, or is cheating.
    Left { age: f64 },
    /// A collider has been near, but the player is not on one: judged in the world the
    /// client saw.
    Near { age: f64 },
}

impl Hub {
    /// [`Hub::collide`] for a guest that may be on a moving collider: the same answer, with the
    /// collision in the colliders' frames as the client saw them.
    pub(super) fn collide_carried(
        &mut self,
        i: usize,
        to: [f64; 3],
        ctx: &TickCtx,
    ) -> (bool, bool, bool) {
        let plan = self.carry_plan(i, to);
        self.guests.update(i, |g| {
            g.carry_hint = None;
            g.carry_correction = None;
        });
        let Some(plan) = plan else {
            // No collider matters: the player is not carried, if it was.
            self.guests.update(i, |g| {
                g.on_collider = None;
                g.carried_standing = false;
            });
            return self.collide(i, to, ctx);
        };
        match plan {
            Plan::Near { age } => {
                self.guests.update(i, |g| g.carried_standing = false);
                at_age(age, || self.collide(i, to, ctx))
            }
            Plan::Left { age } => {
                let result = at_age(age, || self.collide(i, to, ctx));
                if result.1 {
                    self.carry_reject(i);
                } else {
                    self.guests.update(i, |g| {
                        g.on_collider = None;
                        g.carried_standing = false;
                    });
                }
                result
            }
            Plan::Fits {
                collider,
                attached,
                fit,
            } => {
                let from = match self.guests[i].on_collider {
                    Some(held) if attached => fit.frame.to_world(held.local),
                    _ => Vec3::new(
                        self.guests[i].position[0],
                        self.guests[i].position[1],
                        self.guests[i].position[2],
                    ),
                };
                let from = [from.x, from.y, from.z];
                let (wrongly, refused, stands) =
                    at_age(fit.age, || self.collide_from(i, from, to, ctx));
                if refused {
                    if attached {
                        self.carry_reject(i);
                    }
                    return (wrongly, refused, stands);
                }
                let standing = fit.footing == Footing::Standing;
                self.guests.update(i, |g| {
                    g.on_collider = Some(Carried {
                        collider,
                        age: fit.age,
                        local: fit.local,
                    });
                    g.carried_standing = standing;
                });
                (wrongly, refused, stands || standing)
            }
        }
    }

    /// Where guest `i`'s report at `to` stands to the level's colliders; `None` when none matters.
    fn carry_plan(&self, i: usize, to: [f64; 3]) -> Option<Plan> {
        let g = &self.guests[i];
        let dimensions = g.dimensions();
        let body = Body {
            half_width: f64::from(dimensions.width) / 2.0,
            height: f64::from(dimensions.height),
        };
        let feet = Vec3::new(to[0], to[1], to[2]);
        let round_trip = g.latency.load(Ordering::Relaxed);
        let ages = |collider: u32| {
            let hint = g
                .carry_hint
                .filter(|(held, _)| *held == collider)
                .map(|(_, ticks)| ticks);
            Ages::new(round_trip, hint)
        };
        with_history(|history| {
            if let Some(held) = g.on_collider {
                let (log, boxes) = history.get(held.collider)?;
                return Some(
                    match judge(
                        log,
                        history.now(),
                        boxes,
                        feet,
                        body,
                        ages(held.collider),
                        false,
                    ) {
                        Some(fit) => Plan::Fits {
                            collider: held.collider,
                            attached: true,
                            fit,
                        },
                        None => Plan::Left { age: held.age },
                    },
                );
            }
            let around = Aabb::new(
                Vec3::new(feet.x - 1.0, feet.y - 0.5, feet.z - 1.0),
                Vec3::new(feet.x + 1.0, feet.y + body.height + 0.5, feet.z + 1.0),
            );
            let near = history.near(around);
            if near.is_empty() {
                return None;
            }
            for collider in near {
                let Some((log, boxes)) = history.get(collider) else {
                    continue;
                };
                if let Some(fit) =
                    judge(log, history.now(), boxes, feet, body, ages(collider), true)
                {
                    return Some(Plan::Fits {
                        collider,
                        attached: false,
                        fit,
                    });
                }
            }
            Some(Plan::Near {
                age: Ages::new(round_trip, None).anchor,
            })
        })
        .flatten()
    }

    /// A rejected report of a carried player: counted, and the correction is where the server
    /// last had the player on the collider, under the collider's transform now.
    fn carry_reject(&mut self, i: usize) {
        let corrected = self.guests[i].on_collider.and_then(|held| {
            with_history(|history| {
                let (log, _) = history.get(held.collider)?;
                let now = log.frame_at(history.now(), 0.0)?.to_world(held.local);
                Some([now.x, now.y, now.z])
            })
            .flatten()
        });
        self.guests.update(i, |g| {
            g.carry_rejects += 1;
            g.carry_correction = corrected;
        });
    }
}
