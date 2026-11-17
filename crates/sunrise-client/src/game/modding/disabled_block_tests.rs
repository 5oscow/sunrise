//! Switching an owner off holds scheduled work instead of consuming it.
#[cfg(test)]
mod tests {
    use mod_api::{
        BlockPart, Hooks, LevelWriteHost, Registrar, RegistrationContext, TickBlockEvent,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use sunrise_core::{position::BlockPos, ticks::TickPriority};
    use sunrise_world::level::LevelAccess;

    struct Machine(Arc<AtomicUsize>);
    #[mod_api::sunrise_part]
    impl BlockPart for Machine {
        fn tick_block(&self, _: &mut dyn LevelWriteHost, _: &mut TickBlockEvent) -> mod_api::Flow {
            self.0.fetch_add(1, Ordering::Relaxed);
            mod_api::Flow::Cancel
        }
    }
    struct Owner(Arc<AtomicUsize>);
    #[mod_api::__subscriptions]
    impl Hooks for Owner {
        fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
            let block = r.block_key("inert_test:machine");
            r.block_part(block, Machine(self.0.clone()));
        }
        fn blocks(&self, _: &RegistrationContext, e: &mut mod_api::BlocksEvent<'_>) {
            e.blocks.push(
                mod_api::Block::new("inert_test:machine", "minecraft:observer").with_item(false),
            );
        }
    }

    #[test]
    #[ignore = "daily"]
    fn reenabled_owner_resumes_held_scheduled_tick() {
        const NAME: &str =
            "game::disabled_block_tests::tests::reenabled_owner_resumes_held_scheduled_tick";
        if super::super::native_entity_refs::tests::run_fresh_cases(
            NAME,
            "SUNRISE_INERT_BLOCK_CHILD",
            &["valid"],
        ) {
            return;
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "inert_test",
            Box::new(Owner(calls.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods.clone(), || {
            crate::game::api_values::install_parts(&mods).unwrap();
            mod_api::__private::mark_game_thread();
            let base =
                std::path::PathBuf::from(std::env::var_os("SUNRISE_TEST_RESOURCES").unwrap());
            let root = super::super::mods::content_root(&base, &Default::default()).unwrap();
            super::super::native_entity_refs::tests::with_game_at(
                root.prepared().to_owned(),
                |game| {
                    game.options.dedicated = Some(Vec::new());
                    game.native_level_opened(super::super::Dim::Overworld);
                    for x in -1..=1 {
                        for z in -1..=1 {
                            let mut chunk = sunrise_client::chunk::Chunk::empty(x, z);
                            chunk.lit = true;
                            game.world.chunks.insert((x, z), Arc::new(chunk));
                        }
                    }
                    game.tickets
                        .update(Some(sunrise_client::ticket_levels::Sources {
                            forced: vec![(0, 0)],
                            ..Default::default()
                        }));
                    let pos = BlockPos::new(3, 64, 5);
                    let state = game
                        .pack
                        .assets
                        .registry
                        .default_state("inert_test:machine")
                        .unwrap();
                    game.world.set(pos.x, pos.y, pos.z, state).unwrap();
                    game.with_level(|b, level, ticks| {
                        ticks
                            .add_chunk(
                                sunrise_core::position::ChunkPos::new(0, 0),
                                Default::default(),
                                Default::default(),
                            )
                            .unwrap();
                        ticks.schedule_block(
                            level.game_time(),
                            pos,
                            b.block(state),
                            0,
                            TickPriority::High,
                        );
                    });
                    assert!(mod_api::__private::set_mod_enabled("inert_test", false));
                    for _ in 0..3 {
                        game.with_level(|b, level, ticks| b.tick_scheduled(level, ticks));
                        assert_eq!(calls.load(Ordering::Relaxed), 0);
                        assert_eq!(game.world.block(pos.x, pos.y, pos.z), state);
                    }
                    assert!(mod_api::__private::set_mod_enabled("inert_test", true));
                    game.with_level(|b, level, ticks| b.tick_scheduled(level, ticks));
                    assert_eq!(calls.load(Ordering::Relaxed), 1);
                    game.with_level(|b, level, ticks| b.tick_scheduled(level, ticks));
                    assert_eq!(
                        calls.load(Ordering::Relaxed),
                        1,
                        "the held tick runs exactly once"
                    );
                },
            );
        });
    }
}
