use super::*;

impl<R: RandomSource> LootContext<'_, R> {
    pub(super) fn stock_condition(&self, c: &Value) -> Option<bool> {
        if !matches!(
            c["type"].as_str(),
            Some("neoforge:loot_table_id" | "neoforge:can_item_perform_ability")
        ) {
            return None;
        }
        let tool = (c["type"].as_str() == Some("neoforge:can_item_perform_ability"))
            .then(|| {
                self.held
                    .filter(|s| s.count > 0 && &*s.item != "minecraft:air")
                    .map(loot_of)
            })
            .flatten();
        sunrise_world::loot::query::condition(c, &self.query, tool.as_ref(), |item, ability| {
            self.items.vanilla_can_perform(item, ability)
        })
    }
    /// Non-raw completion splits after table functions, retaining the caller's
    /// context and random. Nested entries evaluate raw and never complete a query.
    pub(super) fn roll_root(
        &self,
        id: Option<&str>,
        table: &Value,
        random: &mut R,
    ) -> Vec<ItemStack> {
        let items = self.items.split_loot_stacks(self.evaluate(table, random));
        self.query.finish(id);
        items
    }
    /// The context a mod's own loot kind sees in this engine: the block and
    /// the tool it has, and a seed from the roll's random. The entity, the
    /// damage, the player and the biome belong to the main loot engine's
    /// rolls; they are None here (docs/modding/src/how-to/native-mods.md).
    pub(super) fn native_loot_context(
        &self,
        random: &mut R,
    ) -> sunrise_world::loot::LootContext<'_> {
        sunrise_world::loot::LootContext {
            queried_table_id: self.query.id(),
            queried_stack: self.query.queried_stack.clone(),
            entity: None,
            damage: None,
            player: None,
            block: (!self.block.is_empty()).then_some(self.block),
            tool: self.held.map(|h| sunrise_world::loot::LootTool {
                item: h.item.to_string(),
                enchantments: h
                    .enchantments()
                    .iter()
                    .map(|(id, lvl)| (id.to_string(), *lvl as i32))
                    .collect(),
            }),
            luck: 0.0,
            biome: None,
            seed: random.next_long(),
            place: sunrise_world::loot::current_native_loot_place(),
        }
    }
}

impl ItemStack {
    /// Convert loot facts using the same component write path as native loot functions.
    pub fn from_loot(value: &sunrise_world::loot::LootStack) -> Self {
        let mut stack = Self {
            item: Arc::from(value.item.as_str()),
            count: 1,
            damage: 0,
            extra: None,
        };
        let before = loot_of(&stack);
        apply_loot(&before, value, &mut stack);
        stack
    }
}

impl Items {
    /// `LootTable.createStackSplitter`, also used after resolving dynamic drops.
    pub(super) fn split_loot_stacks(&self, stacks: Vec<ItemStack>) -> Vec<ItemStack> {
        // Stack limits are at least one: singles and empty positions need no lookup.
        if !stacks.iter().any(|stack| {
            stack.count > 1
                && !stack.item.starts_with(DYNAMIC)
                && stack.count > crate::bundle::max_stack(self, stack)
        }) {
            return stacks;
        }
        let mut out = Vec::with_capacity(stacks.len());
        for stack in stacks {
            // A dynamic marker is not an item. Its supplied stacks are split
            // when block_drops resolves it, never the marker's own count.
            if stack.count <= 1 || stack.item.starts_with(DYNAMIC) {
                out.push(stack);
                continue;
            }
            let max = crate::bundle::max_stack(self, &stack);
            let mut count = stack.count;
            while count > max {
                let mut part = stack.clone();
                part.count = max;
                out.push(part);
                count -= max;
            }
            out.push(ItemStack { count, ..stack });
        }
        out
    }

    pub(super) fn scoped_block_roll(
        &self,
        registry: &BlockRegistry,
        state: BlockStateId,
        roll: BlockLootRoll<'_>,
        random: &mut impl RandomSource,
        sequences: Option<(&mut crate::random_sequences::RandomSequences, i64)>,
    ) -> Vec<ItemStack> {
        let Some(block) = registry.state(state) else {
            return vec![];
        };
        let table = self.block_loot_table(registry, state);
        let Some(id) = table else {
            return vec![];
        };
        let Some(table) = self.loot.get(&id) else {
            return vec![];
        };
        sunrise_world::loot::query::with_query(sunrise_world::loot::query::current_query(), || {
            // LootTable.getRandomItems draws from the table's random sequence.
            if let (Some(seq), Some(sequences)) = (
                table.get("random_sequence").and_then(Value::as_str),
                sequences,
            ) {
                let random = sequences.0.get(seq, sequences.1);
                return self.roll_block_table(
                    &id,
                    table,
                    &block.name,
                    &block.properties,
                    roll,
                    random,
                );
            }
            self.roll_block_table(&id, table, &block.name, &block.properties, roll, random)
        })
    }
    /// `Block.dropFromBlockInteractLootTable`: the `block_interact` table `id`
    /// for a right click on `state` (its `match_block` conditions see the
    /// state), from the table's random sequence when it has one.
    pub fn interact_loot(
        &self,
        id: &str,
        registry: &BlockRegistry,
        state: BlockStateId,
        random: &mut impl RandomSource,
        sequences: &mut crate::random_sequences::RandomSequences,
        seed: i64,
    ) -> Vec<ItemStack> {
        let (Some(table), Some(block)) = (self.loot.get(id), registry.state(state)) else {
            return vec![];
        };
        sunrise_world::loot::query::with_query(sunrise_world::loot::query::current_query(), || {
            match table.get("random_sequence").and_then(Value::as_str) {
                Some(sequence) => {
                    let random = sequences.get(sequence, seed);
                    self.interact_roll(id, table, &block.name, &block.properties, random)
                }
                None => self.interact_roll(id, table, &block.name, &block.properties, random),
            }
        })
    }
}

#[cfg(test)]
#[path = "loot_query_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "loot_split_tests.rs"]
mod split_tests;
