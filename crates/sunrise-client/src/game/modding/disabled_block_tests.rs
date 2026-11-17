//! Switching an owner off holds scheduled work and prevents template writes.
#[cfg(test)]
mod tests {
    use super::super::{
        Dim, Game, api_values::TryIntoEngine, native_entity_refs::tests as fixture,
    };
    use mod_api::{
        BlockPart, BlockToolModificationEvent, DispenseMethodEvent, Dispensed, Flow, Hooks, Host,
        LevelWriteHost, Registrar, RegistrationContext, ServerHost, TickBlockEvent,
        ToolModifiedStateEvent,
    };
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use sunrise_client::{chunk::Chunk, items::ItemStack};
    use sunrise_core::{geometry::Direction, position::BlockPos, ticks::TickPriority};
    use sunrise_world::{level::LevelAccess, registry::BlockStateId};

    const POS: BlockPos = BlockPos::new(3, 64, 5);

    struct Machine(Arc<AtomicUsize>);
    #[mod_api::sunrise_part]
    impl BlockPart for Machine {
        fn tick_block(&self, _: &mut dyn LevelWriteHost, _: &mut TickBlockEvent) -> Flow {
            self.0.fetch_add(1, Ordering::Relaxed);
            Flow::Cancel
        }
        fn dispense_method(
            &self,
            _: &mut dyn ServerHost,
            event: &mut DispenseMethodEvent<'_>,
        ) -> Option<Dispensed> {
            Some((Some(event.stack.clone()), 0))
        }
        fn tool_modified_state(&self, _: &dyn Host, _: &mut ToolModifiedStateEvent<'_>) -> Flow {
            Flow::Continue
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
    struct Observer(Arc<Mutex<Vec<Option<BlockStateId>>>>);
    #[mod_api::__subscriptions]
    impl Hooks for Observer {
        fn block_tool_modification(
            &self,
            _: &dyn Host,
            event: &mut BlockToolModificationEvent<'_>,
        ) -> Flow {
            self.0.lock().unwrap().push(
                event
                    .final_state
                    .map(|state| state.try_into_engine(&()).unwrap()),
            );
            Flow::Continue
        }
    }

    fn with_inert_block(
        name: &str,
        test: impl FnOnce(&mut Game<'_>, BlockStateId, &AtomicUsize, &Mutex<Vec<Option<BlockStateId>>>),
    ) {
        let name = format!("game::disabled_block_tests::tests::{name}");
        if fixture::run_fresh_cases(&name, "SUNRISE_INERT_BLOCK_CHILD", &["valid"]) {
            return;
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let candidates = Arc::new(Mutex::new(Vec::new()));
        let mods: mod_api::Mods = vec![
            mod_api::ModEntry::new("inert_test", Box::new(Owner(calls.clone()))),
            mod_api::ModEntry::new("enabled_observer", Box::new(Observer(candidates.clone()))),
        ]
        .into();
        mod_platform::testing::with_mods(mods.clone(), || {
            crate::game::api_values::install_parts(&mods).unwrap();
            mod_api::__private::mark_game_thread();
            let base =
                std::path::PathBuf::from(std::env::var_os("SUNRISE_TEST_RESOURCES").unwrap());
            let root = super::super::mods::content_root(&base, &Default::default()).unwrap();
            fixture::with_game_at(root.prepared().to_owned(), |game| {
                game.options.dedicated = Some(Vec::new());
                game.native_level_opened(Dim::Overworld);
                for x in -1..=1 {
                    for z in -1..=1 {
                        let mut chunk = Chunk::empty(x, z);
                        chunk.lit = true;
                        game.world.chunks.insert((x, z), Arc::new(chunk));
                    }
                }
                let state = game
                    .pack
                    .assets
                    .registry
                    .default_state("inert_test:machine")
                    .unwrap();
                game.world.set(POS.x, POS.y, POS.z, state).unwrap();
                test(game, state, &calls, &candidates);
            });
        });
    }

    #[test]
    #[ignore = "daily"]
    fn reenabled_owner_resumes_held_scheduled_tick() {
        with_inert_block(
            "reenabled_owner_resumes_held_scheduled_tick",
            |game, state, calls, _| {
                game.tickets
                    .update(Some(sunrise_client::ticket_levels::Sources {
                        forced: vec![(0, 0)],
                        ..Default::default()
                    }));
                game.with_level(|_, _, ticks| {
                    ticks
                        .add_chunk(POS.chunk(), Default::default(), Default::default())
                        .unwrap();
                });
                for (before, stepped) in [false, true].into_iter().enumerate() {
                    let tick = |game: &mut Game<'_>| {
                        if stepped {
                            game.tick_scheduled().unwrap();
                        } else {
                            game.with_level(|b, level, ticks| b.tick_scheduled(level, ticks));
                        }
                    };
                    game.with_level(|b, level, ticks| {
                        ticks.schedule_block(
                            level.game_time(),
                            POS,
                            b.block(state),
                            0,
                            TickPriority::High,
                        );
                    });
                    assert!(mod_api::__private::set_mod_enabled("inert_test", false));
                    for _ in 0..3 {
                        tick(game);
                        assert_eq!(calls.load(Ordering::Relaxed), before);
                        assert_eq!(game.world.block(POS.x, POS.y, POS.z), state);
                    }
                    assert!(mod_api::__private::set_mod_enabled("inert_test", true));
                    tick(game);
                    assert_eq!(calls.load(Ordering::Relaxed), before + 1);
                    assert_eq!(game.world.block(POS.x, POS.y, POS.z), state);
                    tick(game);
                    assert_eq!(
                        calls.load(Ordering::Relaxed),
                        before + 1,
                        "the held tick runs exactly once"
                    );
                    assert_eq!(game.world.block(POS.x, POS.y, POS.z), state);
                }
            },
        );
    }

    #[test]
    #[ignore = "daily"]
    fn inert_dispense_keeps_a_legacy_over_max_stack_without_events() {
        with_inert_block(
            "inert_dispense_keeps_a_legacy_over_max_stack_without_events",
            |game, _, _, _| {
                let dispenser = (POS.x - 1, POS.y, POS.z);
                let stack = ItemStack::new("minecraft:stick", 65); // legal persisted count, above 64
                game.audio.capture = Some(Vec::new());
                let particles = game.particles.len();
                assert!(mod_api::__private::set_mod_enabled("inert_test", false));
                assert_eq!(
                    game.dispense_item(dispenser, Direction::East, stack.clone())
                        .unwrap(),
                    Some(stack.clone())
                );
                assert!(game.dropped.is_empty());
                assert!(game.audio.capture.as_ref().unwrap().is_empty());
                assert_eq!(game.particles.len(), particles);

                assert!(mod_api::__private::set_mod_enabled("inert_test", true));
                let left = game
                    .dispense_item(dispenser, Direction::East, stack)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    left.count, 64,
                    "an enabled listener's over-max reply is still refused"
                );
                assert_eq!(game.dropped.len(), 1);
                assert_eq!(game.dropped[0].stack, ItemStack::new("minecraft:stick", 1));
            },
        );
    }

    #[test]
    #[ignore = "daily"]
    fn inert_tool_keeps_block_but_global_listener_sees_vanilla_candidate() {
        with_inert_block(
            "inert_tool_keeps_block_but_global_listener_sees_vanilla_candidate",
            |game, state, _, candidates| {
                let expected = game
                    .pack
                    .assets
                    .registry
                    .default_state("minecraft:stripped_oak_log")
                    .unwrap();
                let tool = mod_api::ToolUse::from(mod_api::__private::built::ToolUse {
                    player: None,
                    hand: mod_api::Hand::Main,
                    pos: [POS.x, POS.y, POS.z],
                    face: [0, 1, 0],
                    click: [3.5, 65.0, 5.5],
                    ability: mod_api::ToolAbility::AxeStrip,
                    simulated: false,
                });
                assert!(mod_api::__private::set_mod_enabled("inert_test", false));
                let mut candidate = Some(expected);
                let mut cancel = false;
                game.mod_tool_modification(
                    &ItemStack::new("minecraft:iron_axe", 1),
                    state,
                    &tool,
                    &mut candidate,
                    &mut cancel,
                );
                assert_eq!(*candidates.lock().unwrap(), [Some(expected)]);
                assert!(
                    cancel,
                    "the caller must not apply the candidate to the disabled block"
                );
                assert_eq!(game.world.block(POS.x, POS.y, POS.z), state);
            },
        );
    }
}
