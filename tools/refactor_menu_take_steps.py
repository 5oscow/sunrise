#!/usr/bin/env python3
"""Prepare Slot.safeTake for an owned take continuation, without changing behavior."""
from pathlib import Path

path = Path('crates/sunrise-client/src/menu.rs')
source = path.read_text()
old = '''        self.touch();
        let mut result = self.try_remove(env, s, amount, max_amount)?;
        self.on_take(env, s, &mut result);
        if self.state.slots[s].role == Role::CartoResult {
            Self::map_crafted(env.maps, &mut result);
        }
        Some(result)
'''
new = '''        self.safe_take_step(env, s, amount, max_amount).finish(self, env)
    }

    fn safe_take_step(
        &mut self,
        env: &Env,
        s: usize,
        amount: u32,
        max_amount: u32,
    ) -> MenuStep<Option<ItemStack>> {
        self.touch();
        let Some(result) = self.try_remove(env, s, amount, max_amount) else {
            return MenuStep::Done(None);
        };
        self.on_take_step(env, s, result).then(self, env, move |menu, env, mut result| {
            if menu.state.slots[s].role == Role::CartoResult {
                Self::map_crafted(env.maps, &mut result);
            }
            MenuStep::Done(Some(result))
        })
'''
assert source.count(old) == 1
source = source.replace(old, new)
assert source.count('mod crafting_take;') == 1
source = source.replace('mod crafting_take;', 'mod crafting_take;\nmod step;\npub use step::{MenuStep, TakeEvent};')
path.write_text(source)

path = Path('crates/sunrise-client/src/menu/crafting_take.rs')
source = path.read_text()
old = 'impl Menu {\n'
new = '''impl Menu {
    fn on_take_step(&mut self, env: &Env, s: usize, mut taken: ItemStack) -> MenuStep<ItemStack> {
        self.on_take(env, s, &mut taken);
        MenuStep::Done(taken)
    }

'''
assert source.count(old) == 1
path.write_text(source.replace(old, new))
