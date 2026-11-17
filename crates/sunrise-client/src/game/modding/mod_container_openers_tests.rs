//! ContainerOpenersCounter timing, including lost openers.
#[cfg(test)]
mod tests {
    use mod_api::{Hooks, ModContainerViewersEvent, ModEntry, ServerHost, VanillaGameEventEvent};
    use std::sync::{Arc, Mutex};
    use sunrise_client::containers::{
        BlockEntity, ModBlockEntity, ModBlockEntityFields, ModContainer,
    };
    use sunrise_client::lan::menu::Slots;
    use sunrise_core::nbt::Tag;

    #[test]
    #[ignore = "daily"]
    fn mod_openers_change_synchronously_and_recheck_five_ticks_later() {
        type Notices = Arc<Mutex<Vec<(u32, u32)>>>;
        type Events = Arc<Mutex<Vec<(String, Option<u32>)>>>;
        struct Listener(Notices, Events);
        #[mod_api::__subscriptions]
        impl Hooks for Listener {
            fn mod_container_viewers(
                &self,
                _: &mut dyn ServerHost,
                e: &mut ModContainerViewersEvent,
            ) {
                self.0.lock().unwrap().push((e.previous, e.viewers));
            }
            fn vanilla_game_event(
                &self,
                _: &mut dyn ServerHost,
                e: &mut VanillaGameEventEvent,
            ) -> mod_api::Flow {
                if e.event == "minecraft:container_open" || e.event == "minecraft:container_close" {
                    self.1.lock().unwrap().push((e.event.to_owned(), e.cause));
                }
                mod_api::Flow::Continue
            }
        }
        let notices = Arc::new(Mutex::new(Vec::new()));
        let events = Arc::new(Mutex::new(Vec::new()));
        let mods = vec![ModEntry::new(
            "openers_test",
            Box::new(Listener(notices.clone(), events.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            super::super::native_guest_turn_tests::with_guests(|game| {
                let pos = (3, 64, 5);
                game.options.dedicated = Some(Vec::new());
                game.world
                    .chunks
                    .insert((0, 0), Arc::new(sunrise_client::chunk::Chunk::empty(0, 0)));
                let state = game
                    .pack
                    .assets
                    .registry
                    .default_state("minecraft:stone")
                    .unwrap();
                game.world.set(pos.0, pos.1, pos.2, state).unwrap();
                game.world.block_entities.insert(
                    pos,
                    BlockEntity::Mod(Box::new(ModBlockEntity::new(
                        "openers_test:box".into(),
                        ModBlockEntityFields {
                            items: vec![None; 9],
                            ..Default::default()
                        },
                    ))),
                );
                game.world.block_entities.set_mod_containers(
                    [(
                        state.0,
                        Arc::new(ModContainer {
                            slots: 9,
                            menu: Some("minecraft:generic_3x3".into()),
                            ..Default::default()
                        }),
                    )]
                    .into_iter()
                    .collect(),
                );
                let start = game.world.game_time;
                for conn in [12, 13] {
                    game.lan.as_mut().unwrap().hub.open_container(
                        conn,
                        "minecraft:generic_3x3",
                        Tag::String("box".into()),
                        pos,
                        Slots::Block(pos),
                        vec![None; 9],
                        None,
                    );
                    game.player_menu_opening(conn);
                }
                assert_eq!(*notices.lock().unwrap(), [(0, 1), (1, 2)]);
                assert_eq!(
                    *events.lock().unwrap(),
                    [("minecraft:container_open".into(), Some(12))]
                );
                game.guest_disconnected(12);
                assert_eq!(notices.lock().unwrap().last(), Some(&(2, 1)));
                // A lost menu is repaired by the scheduled recheck, not each tick.
                game.lan.as_mut().unwrap().hub.guests.update_all(|g| {
                    if g.conn == 13 {
                        g.menu = None;
                    }
                });
                game.world.game_time = start + 4;
                game.tick_mod_container_viewers();
                assert_eq!(notices.lock().unwrap().len(), 3);
                game.world.game_time = start + 5;
                game.tick_mod_container_viewers();
                assert_eq!(notices.lock().unwrap().last(), Some(&(1, 0)));
                assert_eq!(
                    events.lock().unwrap().last(),
                    Some(&("minecraft:container_close".into(), None))
                );
                assert!(game.mod_container_viewers.is_empty());
            });
        });
    }
}
