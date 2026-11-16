#!/usr/bin/env python3
"""Preserve bundle click behavior while propagating the safeTake continuation."""
from pathlib import Path
path = Path('crates/sunrise-client/src/menu/bundle.rs')
s = path.read_text().replace('use super::{Env, Menu, tidy};', 'use super::{Env, Menu, MenuStep, tidy};')
a = s.index('    pub(super) fn bundle_click(')
b = s.index('    /// `BundleItem.overrideOtherStackedOnMe`', a)
s = s[:a] + '''    pub(super) fn bundle_click(&mut self, env: &Env, s: usize, primary: bool) -> bool {
        self.bundle_click_step(env, s, primary).finish(self, env)
    }

    pub(super) fn bundle_click_step(&mut self, env: &Env, s: usize, primary: bool) -> MenuStep<bool> {
        self.touch();
        self.carried_bundle_click_step(env, s, primary).then(self, env, move |menu, env, took| {
            let took = took || menu.clicked_bundle_click(env, s, primary);
            if took {
                // broadcastChangesOnContainerMenu: the open menu's
                // slotsChanged for the player's inventory.
                menu.inventory_slots_changed(env);
            }
            MenuStep::Done(took)
        })
    }

    /// `BundleItem.overrideStackedOnOther`: the carried bundle on a slot.
    fn carried_bundle_click_step(&mut self, env: &Env, s: usize, primary: bool) -> MenuStep<bool> {
        self.touch();
        let Some(mut contents) = self.carried.as_ref().and_then(|c| Contents::of(env.items, c)) else {
            return MenuStep::Done(false);
        };
        let transfer = match (self.slot(s).cloned(), primary) {
            (Some(other), true) => {
                // tryTransfer: what fits, taken by the slot's rules.
                if can_be_in_bundle(env.loop_context, &other) {
                    let room = contents.room_for(env.items, &other);
                    self.safe_take_step(env, s, other.count, room).then(self, env, move |menu, env, taken| {
                        // `tryInsert` asks `canItemBeInBundle` again: the answer above.
                        let moved = taken.map_or(0, |mut taken| contents.try_insert(env.items, &mut taken, true));
                        menu.state.player_sounds.push(if moved > 0 { INSERT } else { INSERT_FAIL });
                        MenuStep::Done(contents)
                    })
                } else {
                    self.state.player_sounds.push(INSERT_FAIL);
                    MenuStep::Done(contents)
                }
            }
            (None, false) => {
                if let Some(stack) = contents.remove_one(env.items) {
                    let count = stack.count;
                    let mut remainder = Some(stack);
                    self.safe_insert(env, s, &mut remainder, count);
                    match remainder {
                        Some(mut rest) => {
                            let fits = fits_inside_container_items(env.loop_context, &rest);
                            contents.try_insert(env.items, &mut rest, fits);
                        }
                        None => self.state.player_sounds.push(REMOVE_ONE),
                    }
                }
                MenuStep::Done(contents)
            }
            _ => return MenuStep::Done(false),
        };
        transfer.then(self, env, |menu, _, contents| {
            if let Some(carried) = menu.state.carried.as_mut() {
                contents.store(carried);
            }
            MenuStep::Done(true)
        })
    }

''' + s[b:]
path.write_text(s)
