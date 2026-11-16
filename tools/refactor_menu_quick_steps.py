#!/usr/bin/env python3
"""Propagate owned take continuations through quickMoveStack without changing behavior."""
from pathlib import Path

path = Path('crates/sunrise-client/src/menu/place.rs')
source = path.read_text()
a = source.index('    pub(super) fn take_after_quick_move(')
b = source.index('\n    fn place_grid(', a)
old = source[a:b]
new = old.replace('fn take_after_quick_move(', 'fn take_after_quick_move_step(')
new = new.replace('clicked: &mut ItemStack,', 'mut clicked: ItemStack,')
new = new.replace('stack: &mut ItemStack,', 'stack: ItemStack,')
new = new.replace('    ) {', '    ) -> MenuStep<(ItemStack, ItemStack)> {', 1)
new = new.replace('return self.on_take(env, index, stack);', '''return self.on_take_step(env, index, stack).then(self, env, move |_, _, stack| {
                MenuStep::Done((clicked, stack))
            });''')
new = new.replace('''        self.on_take(env, index, clicked);
        clicked.count += left;
''', '''        self.on_take_step(env, index, clicked).then(self, env, move |_, _, mut clicked| {
            clicked.count += left;
            MenuStep::Done((clicked, stack))
        })
''')
assert new != old
source = source[:a] + new + source[b:]
path.write_text(source)

path = Path('crates/sunrise-client/src/menu.rs')
source = path.read_text()
a = source.index('    fn quick_move(&mut self, env: &Env, index: usize) -> Option<ItemStack> {')
b = source.index('\n}\n\n/// `MerchantMenu`', a)
old = source[a:b]
new = old.replace('    fn quick_move(', '    fn quick_move_step(', 1)
new = new.replace('-> Option<ItemStack> {', '-> MenuStep<Option<ItemStack>> {', 1)
new = new.replace('let mut stack = self.slot(index)?.clone();', '''let Some(mut stack) = self.slot(index).cloned() else {
            return MenuStep::Done(None);
        };''')
new = new.replace('return None;', 'return MenuStep::Done(None);')
new = new.replace('''        if after_take || matches!(self.state.kind, Kind::Inventory | Kind::Crafting) {
''', '''        let take = if after_take || matches!(self.state.kind, Kind::Inventory | Kind::Crafting) {
''')
new = new.replace('''            self.take_after_quick_move(env, index, &mut clicked, &mut stack);
        }
        // What the result slot's move left over is dropped.
''', '''            self.take_after_quick_move_step(env, index, clicked, stack)
        } else {
            MenuStep::Done((clicked, stack))
        };
        take.then(self, env, move |menu, _, (clicked, stack)| {
        // What the result slot's move left over is dropped.
''')
a_tail = new.index('        // What the result slot\'s move left over is dropped.')
head, tail = new[:a_tail], new[a_tail:]
tail = tail.replace('self.', 'menu.').replace('        Some(clicked)\n    }', '        MenuStep::Done(Some(clicked))\n        })\n    }')
new = head + tail
wrapper = '''    fn quick_move(&mut self, env: &Env, index: usize) -> Option<ItemStack> {
        self.quick_move_step(env, index).finish(self, env)
    }

'''
source = source[:a] + wrapper + new + source[b:]
path.write_text(source)
