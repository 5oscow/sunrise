//! Whether a tool is correct for a block, for native mods (`LevelRef::block_correct_tool`):
//! the tool half of vanilla's harvest check, `ItemStack.isCorrectToolForDrops`, read at a
//! position as `level_block_break_speed` reads the tool's speed. Whether the block needs a
//! correct tool at all is the state read `block_state_requires_correct_tool`.
use super::Game;
use mod_api::LevelRef;
use sunrise_items::items::ItemStack;

impl Game<'_> {
    /// `ItemStack.isCorrectToolForDrops` of `tool` on the block at `pos`. None for a stale
    /// handle or an unloaded or out-of-height position.
    pub(super) fn level_block_correct_tool(
        &self,
        level: LevelRef,
        pos: [i32; 3],
        tool: Option<&ItemStack>,
    ) -> Option<bool> {
        let world = self.native_level_read_world(level)?;
        if !world.loaded(pos[0], pos[2]) || !world.shape.contains_y(pos[1]) {
            return None;
        }
        let state = world.block(pos[0], pos[1], pos[2]);
        let name = &self.res.facts().get(state).name;
        Some(self.items().correct_for_drops(tool, name))
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_entity_refs::tests::with_game;
    use sunrise_client::items::ItemStack;

    #[test]
    #[ignore = "daily"]
    fn a_tool_is_correct_for_a_block_as_vanilla_says() {
        with_game(|game| {
            let level = game.level("minecraft:overworld").unwrap();
            let registry = &game.pack.assets.registry;
            let stone = registry.default_state("minecraft:stone").unwrap();
            let dirt = registry.default_state("minecraft:dirt").unwrap();
            let obsidian = registry.default_state("minecraft:obsidian").unwrap();
            // Stone, dirt and obsidian in a row at y 64 of a loaded chunk.
            let mut chunk = sunrise_client::chunk::Chunk::empty(0, 0);
            chunk.set(0, 64, 0, stone.0);
            chunk.set(1, 64, 0, dirt.0);
            chunk.set(2, 64, 0, obsidian.0);
            game.world.chunks.insert((0, 0), std::sync::Arc::new(chunk));
            assert_eq!(game.world.block(2, 64, 0), obsidian);

            // Stone needs a tool, dirt doesn't.
            assert_eq!(game.block_state_requires_correct_tool(stone), Some(true));
            assert_eq!(game.block_state_requires_correct_tool(dirt), Some(false));

            // The hand and a wrong tool aren't correct for stone; any pickaxe is.
            let wood = ItemStack::new("minecraft:wooden_pickaxe", 1);
            let shovel = ItemStack::new("minecraft:iron_shovel", 1);
            assert_eq!(game.level_block_correct_tool(level, [0, 64, 0], None), Some(false));
            assert_eq!(
                game.level_block_correct_tool(level, [0, 64, 0], Some(&shovel)),
                Some(false)
            );
            assert_eq!(
                game.level_block_correct_tool(level, [0, 64, 0], Some(&wood)),
                Some(true)
            );
            // A shovel is correct for dirt (it is in `mineable/shovel`).
            assert_eq!(
                game.level_block_correct_tool(level, [1, 64, 0], Some(&shovel)),
                Some(true)
            );
            // Tiers: wood is not enough for obsidian, diamond is.
            let diamond = ItemStack::new("minecraft:diamond_pickaxe", 1);
            assert_eq!(
                game.level_block_correct_tool(level, [2, 64, 0], Some(&wood)),
                Some(false)
            );
            assert_eq!(
                game.level_block_correct_tool(level, [2, 64, 0], Some(&diamond)),
                Some(true)
            );

            // A `minecraft:tool` component decides on its own: a stick made correct for stone.
            let mut custom = ItemStack::new("minecraft:stick", 1);
            let components = sunrise_client::item_data::command_components(
                r#"[tool={rules:[{blocks:"stone",correct_for_drops:true}]}]"#,
            )
            .unwrap();
            (custom.damage, custom.extra) =
                sunrise_client::item_data::decode_components(Some(&components));
            assert_eq!(
                game.level_block_correct_tool(level, [0, 64, 0], Some(&custom)),
                Some(true)
            );

            // Out of height, there is no answer.
            assert_eq!(game.level_block_correct_tool(level, [0, 10_000, 0], None), None);
        });
    }
}
