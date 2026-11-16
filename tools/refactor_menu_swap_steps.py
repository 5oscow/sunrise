#!/usr/bin/env python3
"""Propagate owned continuations through number-key swaps without changing behavior."""
from pathlib import Path
path = Path('crates/sunrise-client/src/menu/click.rs')
s = path.read_text()
a = s.index('    pub(super) fn swap(')
b = s.index('    /// `AbstractContainerMenu.canTakeItemForPickAll`', a) if '    /// `AbstractContainerMenu.canTakeItemForPickAll`' in s[a:] else s.index('    pub fn can_take_for_pick_all(', a)
old = s[a:b]
new = old.replace('fn swap(', 'fn swap_step(', 1).replace('button: usize) {', 'button: usize) -> MenuStep<()> {', 1)
new = new.replace('return;', 'return MenuStep::Done(());')
new = new.replace('''        if self.swap_equip_source(env, target, inv_slot) {
            return MenuStep::Done(());
        }
''', '''        self.swap_equip_source_step(env, target, inv_slot).then(self, env, move |menu, env, handled| {
            if handled { MenuStep::Done(()) } else { menu.swap_plain_step(env, target, inv_slot) }
        })
    }

    fn swap_plain_step(&mut self, env: &Env, target: usize, inv_slot: usize) -> MenuStep<()> {
''')
new = new.replace('''                    self.on_take(env, target, &mut taken);
                    if self.state.slots[target].role == Role::CraftResult {
                        self.state.containers[PLAYER][inv_slot] = Some(taken);
                    } else if self.state.slots[target].role == Role::CartoResult
                        && let Some(stack) = self.state.containers[PLAYER][inv_slot].as_mut()
                    {
                        Self::map_crafted(env.maps, stack);
                    }
''', '''                    return self.on_take_step(env, target, taken).then(self, env, move |menu, env, taken| {
                        if menu.state.slots[target].role == Role::CraftResult {
                            menu.state.containers[PLAYER][inv_slot] = Some(taken);
                        } else if menu.state.slots[target].role == Role::CartoResult
                            && let Some(stack) = menu.state.containers[PLAYER][inv_slot].as_mut()
                        {
                            Self::map_crafted(env.maps, stack);
                        }
                        MenuStep::Done(())
                    });
''')
new = new.replace('''                        self.on_take(env, target, &mut taken);
                        if !self.inventory_add(env, &mut taken) {
                            self.drop_stack(&taken);
                        }
''', '''                        return self.on_take_step(env, target, taken).then(self, env, |menu, env, mut taken| {
                            if !menu.inventory_add(env, &mut taken) {
                                menu.drop_stack(&taken);
                            }
                            MenuStep::Done(())
                        });
''')
new = new.replace('''                        self.on_take(env, target, &mut taken);''', '''                        return self.on_take_step(env, target, taken).then(self, env, |_, _, _| MenuStep::Done(()));''')
assert 'self.on_take(' not in new
# Preserve any documentation following swap, adding Done before its closing brace.
end = new.rindex('            (None, None) => {}\n        }\n    }')
new = new[:end] + new[end:].replace('            (None, None) => {}\n        }\n    }', '            (None, None) => {}\n        }\n        MenuStep::Done(())\n    }', 1)
new = new.replace('Some(mut taken)', 'Some(taken)')
wrapper = '''    pub(super) fn swap(&mut self, env: &Env, target: usize, button: usize) {
        self.swap_step(env, target, button).finish(self, env)
    }

'''
path.write_text(s[:a] + wrapper + new + s[b:])
