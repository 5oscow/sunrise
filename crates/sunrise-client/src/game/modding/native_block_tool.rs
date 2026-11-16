//! Whether a block requires a correct tool, for native mods
//! (`BlockStates::requires_correct_tool`); which tool is correct is
//! `BlockStates::is_correct_tool_for_drops`.

#[cfg(test)]
mod tests {
    use super::super::native_entity_refs::tests::with_game;

    #[test]
    #[ignore = "daily"]
    fn a_state_requires_a_correct_tool_as_vanilla_says() {
        with_game(|game| {
            let registry = &game.pack.assets.registry;
            let stone = registry.default_state("minecraft:stone").unwrap();
            let dirt = registry.default_state("minecraft:dirt").unwrap();
            // Stone needs a tool, dirt doesn't.
            assert_eq!(game.block_state_requires_correct_tool(stone), Some(true));
            assert_eq!(game.block_state_requires_correct_tool(dirt), Some(false));
        });
    }
}
