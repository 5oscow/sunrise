#!/usr/bin/env python3
"""Release equipment source borrows before a mod-owned slot's take continuation."""
from pathlib import Path
path = Path('crates/sunrise-client/src/menu/equip_movement.rs')
s = path.read_text()
a = s.index('    pub(super) fn pickup_equip_carried(')
b = s.index('\n}\n\n#[cfg(test)]', a)
old = s[a:b]
new = old.replace('fn pickup_equip_carried(', 'fn pickup_equip_carried_step(').replace('fn swap_equip_source(', 'fn swap_equip_source_step(')
new = new.replace('-> bool {', '-> MenuStep<bool> {')
new = new.replace('return false;', 'return MenuStep::Done(false);').replace('return true;', 'return MenuStep::Done(true);')
new = new.replace('''                    self.on_take(env, s, &mut taken);''', '''                    if self.mod_take_slot(s) {
                        // The equipment source is a local borrow, not an owned continuation.
                        // A mod slot has no built-in onTake override; publish its cursor and
                        // retire the source scope before returning control to the coordinator.
                        self.state.carried = source.remove();
                        tidy(&mut self.state.carried);
                        drop(source);
                        return self.on_take_step(env, s, taken).then(self, env, |_, _, _| MenuStep::Done(true));
                    }
                    self.on_take(env, s, &mut taken);''')
new = new.replace('''                self.on_take(env, target, &mut taken);
                if !self.inventory_add(env, &mut taken) {''', '''                if self.mod_take_slot(target) {
                    drop(source);
                    return self.on_take_step(env, target, taken).then(self, env, |menu, env, mut taken| {
                        if !menu.inventory_add(env, &mut taken) {
                            menu.drop_stack(&taken);
                        }
                        MenuStep::Done(true)
                    });
                }
                self.on_take(env, target, &mut taken);
                if !self.inventory_add(env, &mut taken) {''')
new = new.replace('''                self.on_take(env, target, &mut taken);
            }
        }
        true''', '''                if self.mod_take_slot(target) {
                    drop(source);
                    return self.on_take_step(env, target, taken).then(self, env, |_, _, _| MenuStep::Done(true));
                }
                self.on_take(env, target, &mut taken);
            }
        }
        MenuStep::Done(true)''')
new = new.replace('''        tidy(&mut self.state.carried);
        true''', '''        tidy(&mut self.state.carried);
        MenuStep::Done(true)''')
wrapper = '''    pub(super) fn pickup_equip_carried(&mut self, env: &Env, s: usize, primary: bool) -> bool {
        self.pickup_equip_carried_step(env, s, primary).finish(self, env)
    }
    pub(super) fn swap_equip_source(&mut self, env: &Env, target: usize, inv_slot: usize) -> bool {
        self.swap_equip_source_step(env, target, inv_slot).finish(self, env)
    }
'''
s = s[:a] + wrapper + new + s[b:]
path.write_text(s)

path = Path('crates/sunrise-client/src/menu/crafting_take.rs')
s = path.read_text()
s = s.replace('impl Menu {', '''impl Menu {
    pub(super) fn mod_take_slot(&self, slot: usize) -> bool {
        matches!(self.kind(), Kind::Mod(_))
            && matches!(self.state.slots[slot].role, Role::Mod | Role::ModResult)
    }
''', 1)
path.write_text(s)
