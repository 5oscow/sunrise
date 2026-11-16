#!/usr/bin/env python3
"""Propagate take continuations through click dispatch and its transfer loops."""
from pathlib import Path
path = Path('crates/sunrise-client/src/menu/click.rs')
s = path.read_text()
a = s.index('    pub(super) fn click_planned(')
b = s.index('    /// The PICKUP branch', a)
old = s[a:b]
new = old.replace('fn click_planned(', 'fn click_step(', 1)
new = new.replace(') -> Result<(), &\'static str> {', ') -> MenuStep<Result<(), &\'static str>> {', 1)
new = new.replace('return Err(OUT_OF_BOUNDS);', 'return MenuStep::Done(Err(OUT_OF_BOUNDS));')
new = new.replace('return Ok(());', 'return MenuStep::Done(Ok(()));')
new = new.replace('return self.click(env, s, ty, Input::Pickup);', 'return self.click_step(env, s, ty, Input::Pickup);')
new = new.replace('''                let mut clicked = self.quick_move(env, s);
                while let Some(c) = &clicked {
                    if !self.slot(s).is_some_and(|now| same_item(now, c)) {
                        break;
                    }
                    clicked = self.quick_move(env, s);
                }
''', '''                return MenuStep::repeat(self, env, (), move |menu, env, ()| {
                    menu.quick_move_step(env, s).then(menu, env, move |menu, _, clicked| {
                        MenuStep::Done(if clicked.as_ref().is_some_and(|c| menu.slot(s).is_some_and(|now| same_item(now, c))) {
                            std::ops::ControlFlow::Continue(())
                        } else {
                            std::ops::ControlFlow::Break(Ok(()))
                        })
                    })
                });
''')
new = new.replace('self.pickup(env, slot_index as usize, primary);', 'return self.pickup_step(env, slot_index as usize, primary).then(self, env, |_, _, ()| MenuStep::Done(Ok(())));')
new = new.replace('self.swap(env, slot_index as usize, button as usize);', 'return self.swap_step(env, slot_index as usize, button as usize).then(self, env, |_, _, ()| MenuStep::Done(Ok(())));')
a_throw = new.index('            let mut taken = self.safe_take(')
b_throw = new.index('        } else if input == Input::PickupAll', a_throw)
new = new[:a_throw] + '''            return MenuStep::repeat(self, env, (), move |menu, env, ()| {
                menu.safe_take_step(env, s, amount, u32::MAX).then(menu, env, move |menu, _, taken| {
                    if let Some(taken) = &taken {
                        menu.drop_stack(taken);
                    }
                    MenuStep::Done(if button == 1 && taken.as_ref().is_some_and(|t| menu.slot(s).is_some_and(|now| same_item(now, t))) {
                        std::ops::ControlFlow::Continue(())
                    } else {
                        std::ops::ControlFlow::Break(Ok(()))
                    })
                })
            });
''' + new[b_throw:]
new = new.replace('self.pickup_all(env, slot_index as usize, button);', 'return self.pickup_all_step(env, slot_index as usize, button).then(self, env, |_, _, ()| MenuStep::Done(Ok(())));')
assert new.endswith('        Ok(())\n    }\n\n')
new = new[:-len('        Ok(())\n    }\n\n')] + '        MenuStep::Done(Ok(()))\n    }\n\n'
wrapper = '''    pub(super) fn click_planned(&mut self, env: &Env, slot_index: i32, button: i32, input: Input) -> Result<(), &'static str> {
        self.click_step(env, slot_index, button, input).finish(self, env)
    }

'''
s = s[:a] + wrapper + new + s[b:]
a = s.index('    pub(super) fn pickup_all(')
b = s.index('\n}', a)
old = s[a:b]
# The loop state (pass and next index) is owned; an onTake runs before growing the cursor.
new = '''    pub(super) fn pickup_all_step(&mut self, env: &Env, s: usize, button: i32) -> MenuStep<()> {
        use std::ops::ControlFlow::{Break, Continue};
        self.touch();
        let Some(carried) = self.state.carried.clone() else {
            return MenuStep::Done(());
        };
        if self.has_item(s) && self.may_pickup(env, s) {
            return MenuStep::Done(());
        }
        let total = self.state.slots.len() as i32;
        let (start, step) = if button == 0 { (0, 1) } else { (total - 1, -1) };
        let carried_max = Self::max_stack(env, &carried);
        MenuStep::repeat(self, env, (0, start), move |menu, env, (pass, i)| {
            if pass == 2 { return MenuStep::Done(Break(())); }
            if i < 0 || i >= total || menu.state.carried.as_ref().map_or(0, |c| c.count) >= carried_max {
                return MenuStep::Done(Continue((pass + 1, start)));
            }
            let t = i as usize;
            let next = (pass, i + step);
            let have = menu.state.carried.clone().expect("some");
            if menu.has_item(t)
                && menu.quick_replace(t, &have, env)
                && menu.may_pickup(env, t)
                && menu.can_take_for_pick_all(t)
            {
                let item = menu.slot(t).expect("has item").clone();
                if pass != 0 || item.count != Self::max_stack(env, &item) {
                    let room = carried_max - have.count;
                    return menu.safe_take_step(env, t, item.count, room).then(menu, env, move |menu, _, removed| {
                        if let Some(removed) = removed && let Some(c) = menu.state.carried.as_mut() {
                            c.count += removed.count;
                        }
                        MenuStep::Done(Continue(next))
                    });
                }
            }
            MenuStep::Done(Continue(next))
        })
    }
'''
s = s[:a] + new + s[b:]
# Internal synchronous wrappers are no longer needed: only the public click API completes steps.
for name, end in [('pickup', '    pub(super) fn pickup_step('), ('swap', '    pub(super) fn swap_step(')]:
    a = s.index('    pub(super) fn ' + name + '(')
    b = s.index(end, a)
    s = s[:a] + s[b:]
path.write_text(s)

path = Path('crates/sunrise-client/src/menu/bundle.rs')
s = path.read_text()
a = s.index('    pub(super) fn bundle_click(')
b = s.index('    pub(super) fn bundle_click_step(', a)
path.write_text(s[:a] + s[b:])
path = Path('crates/sunrise-client/src/menu/equip_movement.rs')
s = path.read_text()
a = s.index('    pub(super) fn pickup_equip_carried(')
b = s.index('    pub(super) fn pickup_equip_carried_step(', a)
path.write_text(s[:a] + s[b:])
path = Path('crates/sunrise-client/src/menu.rs')
s = path.read_text()
a = s.index('    fn safe_take(')
b = s.index('    fn safe_take_step(', a)
path.write_text(s[:a] + s[b:])
