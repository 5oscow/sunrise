#!/usr/bin/env python3
"""Propagate owned continuations through the PICKUP branch without changing behavior."""
from pathlib import Path
path = Path('crates/sunrise-client/src/menu/click.rs')
s = path.read_text()
a = s.index('    pub(super) fn pickup(')
b = s.index('    /// The SWAP branch', a)
old = s[a:b]
new = old
new = new.replace('    pub(super) fn pickup(', '    pub(super) fn pickup_step(', 1)
new = new.replace('primary: bool) {', 'primary: bool) -> MenuStep<()> {', 1)
new = new.replace('''        let handled = if canceled {
            handled
        } else {
            self.bundle_click(env, s, primary)
        };
        if !handled {
            self.pickup_inner(env, s, primary);
        }
        self.slot_changed(env, s);
''', '''        let handled = if canceled {
            MenuStep::Done(handled)
        } else {
            self.bundle_click_step(env, s, primary)
        };
        handled.then(self, env, move |menu, env, handled| {
            if handled { MenuStep::Done(()) } else { menu.pickup_inner_step(env, s, primary) }
        }).then(self, env, move |menu, env, ()| {
            menu.slot_changed(env, s);
            MenuStep::Done(())
        })
''')
new = new.replace('''    pub(super) fn pickup_inner(&mut self, env: &Env, s: usize, primary: bool) {
        self.touch();
        if self.pickup_equip_carried(env, s, primary) {
            return;
        }
''', '''    pub(super) fn pickup_inner_step(&mut self, env: &Env, s: usize, primary: bool) -> MenuStep<()> {
        self.touch();
        self.pickup_equip_carried_step(env, s, primary).then(self, env, move |menu, env, handled| {
            if handled { MenuStep::Done(()) } else { menu.pickup_plain_step(env, s, primary) }
        })
    }

    fn pickup_plain_step(&mut self, env: &Env, s: usize, primary: bool) -> MenuStep<()> {
''')
new = new.replace('''                        self.on_take(env, s, &mut taken);
                        if self.state.slots[s].role == Role::CartoResult {
                            Self::map_crafted(env.maps, &mut taken);
                        }
                        if matches!(
                            self.state.slots[s].role,
                            Role::CartoResult | Role::CraftResult
                        ) {
                            self.state.carried = Some(taken);
                        }
''', '''                        return self.on_take_step(env, s, taken).then(self, env, move |menu, env, mut taken| {
                            if menu.state.slots[s].role == Role::CartoResult {
                                Self::map_crafted(env.maps, &mut taken);
                            }
                            if matches!(menu.state.slots[s].role, Role::CartoResult | Role::CraftResult) {
                                menu.state.carried = Some(taken);
                            }
                            MenuStep::Done(())
                        });
''')
new = new.replace('''                            self.on_take(env, s, &mut taken);
                            if self.state.slots[s].role == Role::CartoResult {
                                // onCraftedBy edits the taken stack, not the carried one.
                                Self::map_crafted(env.maps, &mut taken);
                            }
''', '''                            return self.on_take_step(env, s, taken).then(self, env, move |menu, env, mut taken| {
                                if menu.state.slots[s].role == Role::CartoResult {
                                    // onCraftedBy edits the taken stack, not the carried one.
                                    Self::map_crafted(env.maps, &mut taken);
                                }
                                MenuStep::Done(())
                            });
''')
# The plain pickup's match is the final statement; no continuation work follows it.
assert new.endswith('        }\n    }\n\n')
new = new[:-len('        }\n    }\n\n')] + '        }\n        MenuStep::Done(())\n    }\n\n'
# These copies are no longer mutated before entering on_take_step.
new = new.replace('if let Some(mut taken) =', 'if let Some(taken) =')
assert 'self.on_take(' not in new
wrapper = '''    pub(super) fn pickup(&mut self, env: &Env, s: usize, primary: bool) {
        self.pickup_step(env, s, primary).finish(self, env)
    }

'''
s = s[:a] + wrapper + new + s[b:]
path.write_text(s)
