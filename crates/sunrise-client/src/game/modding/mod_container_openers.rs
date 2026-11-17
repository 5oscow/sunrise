//! The server's ContainerOpenersCounter for declared mod containers.
use super::Game;
use sunrise_client::mobs::{Species, Who};
use sunrise_core::geometry::{Aabb, Vec3};

type Pos = (i32, i32, i32);

pub(super) struct ModContainerOpeners {
    pub count: u32,
    recheck: i64,
    range: f64,
}

impl Game<'_> {
    /// startOpen/stopOpen, with the opening player's body in place.
    pub(super) fn mod_container_player_opener(&mut self, pos: Pos, open: bool) {
        if self.spectator() {
            return;
        }
        let range = self.interaction_range("minecraft:block_interaction_range", 4.5, 5.0);
        self.mod_container_opener(pos, open, Some(Who::Player), range);
    }

    pub(super) fn mod_container_opener(
        &mut self,
        pos: Pos,
        open: bool,
        source: Option<Who>,
        range: f64,
    ) {
        if self.mod_menu_at(pos).is_none() {
            return;
        }
        let key = (self.dim.id(), pos);
        let previous = self.mod_container_viewers.get(&key).map_or(0, |c| c.count);
        let count = if open {
            previous.saturating_add(1)
        } else {
            previous.saturating_sub(1)
        };
        if open {
            let counter = self
                .mod_container_viewers
                .entry(key)
                .or_insert(ModContainerOpeners {
                    count: 0,
                    recheck: self.world.game_time + 5,
                    range,
                });
            counter.count = count;
            counter.range = counter.range.max(range);
        } else if count == 0 {
            self.mod_container_viewers.remove(&key);
        } else if let Some(counter) = self.mod_container_viewers.get_mut(&key) {
            counter.count = count;
        }
        self.mod_container_opener_notice(pos, previous, count, source);
    }

    fn mod_container_opener_notice(
        &mut self,
        pos: Pos,
        previous: u32,
        count: u32,
        source: Option<Who>,
    ) {
        let at = [pos.0, pos.1, pos.2];
        if previous == 0 && count > 0 {
            self.post_block_game_event("minecraft:container_open", at, source, None);
        } else if previous > 0 && count == 0 {
            self.post_block_game_event("minecraft:container_close", at, source, None);
        }
        self.native_mod_container_viewers(self.dim.id(), at, count, previous);
    }

    /// Scheduled rechecks only: no player or mob walk without an open container.
    pub(super) fn tick_mod_container_viewers(&mut self) {
        let dim = self.dim.id();
        let time = self.world.game_time;
        let due: Vec<_> = self
            .mod_container_viewers
            .iter()
            .filter(|((d, _), c)| *d == dim && c.recheck <= time)
            .map(|((_, pos), c)| (*pos, c.count, c.range))
            .collect();
        for (pos, previous, range) in due {
            let range = range + 4.0;
            let area = Aabb::new(
                Vec3::new(
                    pos.0 as f64 - range,
                    pos.1 as f64 - range,
                    pos.2 as f64 - range,
                ),
                Vec3::new(
                    pos.0 as f64 + 1.0 + range,
                    pos.1 as f64 + 1.0 + range,
                    pos.2 as f64 + 1.0 + range,
                ),
            );
            let mut count = 0;
            if self.mod_menu_at(pos).is_some() {
                if let Some(host) = &self.lan {
                    count += host
                        .hub
                        .guests
                        .iter()
                        .filter(|g| {
                            host.hub.here(g)
                                && !g.spectator()
                                && g.bounds().intersects(area)
                                && g.menu.as_ref().is_some_and(|m| m.pos == Some(pos))
                        })
                        .count() as u32;
                }
                if self.container == Some(pos)
                    && !self.spectator()
                    && self
                        .player_view()
                        .is_some_and(|p| p.bounds.intersects(area))
                {
                    count += 1;
                }
                count += self
                    .mobs
                    .iter()
                    .filter(|m| {
                        m.species() == Species::CopperGolem
                            && !m.removed
                            && m.extra.opened_chest == Some([pos.0, pos.1, pos.2])
                            && m.bounds().intersects(area)
                    })
                    .count() as u32;
            }
            let key = (dim, pos);
            if count == 0 {
                self.mod_container_viewers.remove(&key);
            } else if let Some(counter) = self.mod_container_viewers.get_mut(&key) {
                counter.count = count;
                counter.recheck = time + 5;
            }
            // recheckOpeners uses no entity source, even on a zero crossing.
            self.mod_container_opener_notice(pos, previous, count, None);
        }
    }
}
