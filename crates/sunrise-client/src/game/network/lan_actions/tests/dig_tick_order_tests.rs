//! ServerPlayer.tick precedes the connection phase, including across levels.
use super::*;
use crate::game::levels::{ServerTickPhase, server_tick_phases};
use crate::game::native_entity_refs::tests::with_game;
use crate::game::native_item_places::tests::{guest, open_to_lan};
use mod_api::{HostSide, ServerHost};
use std::sync::{Arc, Mutex};
use sunrise_client::chunk::Chunk;
use sunrise_world::registry::BlockRegistry;

/// A stub for two levels, each with one connection mining one block. Drive
/// the production dispatcher without a Game or the prepared resource tables.
#[test]
fn every_level_ticks_before_the_connection_phase() {
    let blocks = BlockRegistry::from_facts_json(
        &br#"{"defaults":{"minecraft:air":0,"minecraft:stone":1},"states":[["a",true,false,"","minecraft:air"],["s",false,false,"","minecraft:stone"]]}"#[..],
    )
    .unwrap();
    let stone = blocks.default_state("minecraft:stone").unwrap();
    let air = blocks.default_state("minecraft:air").unwrap();
    let dims = [Dim::Overworld, Dim::Nether];
    let mut dig = [0; 2];
    let mut heard = Vec::new();
    for phase in server_tick_phases(&dims) {
        match phase {
            ServerTickPhase::Level(i, dim) => {
                assert_eq!(dim, dims[i]);
                if stone != air {
                    dig[i] += 1;
                    heard.push(("dig", dim, dig[i]));
                }
            }
            ServerTickPhase::Connections => {
                for (dim, progress) in dims.into_iter().zip(dig) {
                    heard.push(("pre", dim, progress));
                    heard.push(("post", dim, progress));
                }
            }
        }
    }
    assert_eq!(
        heard,
        [
            ("dig", Dim::Overworld, 1),
            ("dig", Dim::Nether, 1),
            ("pre", Dim::Overworld, 1),
            ("post", Dim::Overworld, 1),
            ("pre", Dim::Nether, 1),
            ("post", Dim::Nether, 1),
        ],
    );
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    EntityPre(u32),
    EntityPost(u32),
    Dig(u128, i32),
    PlayerPre(u128, &'static str, Option<i32>, Option<i32>),
    PlayerPost(u128),
}

struct Observe {
    events: Arc<Mutex<Vec<Event>>>,
    cancel: u32,
}

#[mod_api::__subscriptions]
impl mod_api::Hooks for Observe {
    fn register(&self, registrar: &mut dyn mod_api::Registrar, _: &mod_api::RegistrationContext) {
        registrar.entity_tick_each("minecraft:player");
    }
    fn entity_tick_listened(&self, _: HostSide<'_>, event: &mut mod_api::EntityTickListenedEvent) {
        event.listened = true;
    }
    fn entity_tick_pre(&self, _: HostSide<'_>, event: &mut mod_api::EntityTickPreEvent<'_>) {
        for &id in event.entities {
            self.events.lock().unwrap().push(Event::EntityPre(id));
            if id == self.cancel {
                event.canceled.push(id);
            }
        }
    }
    fn entity_tick_post(&self, _: HostSide<'_>, event: &mut mod_api::EntityTickPostEvent<'_>) {
        for &id in event.entities {
            self.events.lock().unwrap().push(Event::EntityPost(id));
        }
    }
    fn block_break_progress_update(
        &self,
        _: &mut dyn ServerHost,
        event: &mut mod_api::BlockBreakProgressUpdateEvent<'_>,
    ) {
        self.events
            .lock()
            .unwrap()
            .push(Event::Dig(event.player, event.progress));
    }
    fn player_tick_pre(&self, host: HostSide<'_>, event: &mut mod_api::PlayerTickPreEvent) {
        let HostSide::Server(host) = host else { return };
        let entity = event.player.entity();
        self.events.lock().unwrap().push(Event::PlayerPre(
            event.player.uuid(),
            host.dimension(),
            entity.invulnerable_time(host),
            entity.damage_cooldown_time(host),
        ));
    }
    fn player_tick_post(&self, _: HostSide<'_>, event: &mut mod_api::PlayerTickPostEvent) {
        self.events
            .lock()
            .unwrap()
            .push(Event::PlayerPost(event.player.uuid()));
    }
}

#[test]
#[ignore = "daily"]
fn player_tick_hooks_observe_advanced_dig_progress_and_skipped_body_still_ticks_game_mode() {
    let heard = Arc::default();
    mod_platform::testing::with_mods(
        vec![mod_api::ModEntry::new(
            "observe",
            Box::new(Observe {
                events: Arc::clone(&heard),
                cancel: 23,
            }),
        )]
        .into(),
        || {
            with_game(|game| {
                game.world_root = game.options.game_dir.clone().unwrap();
                open_to_lan(game);
                game.build_registries();
                let stone = game
                    .pack
                    .assets
                    .registry
                    .default_state("minecraft:stone")
                    .unwrap();
                for (dim, conns) in [(Dim::Overworld, [1, 3]), (Dim::Nether, [2, 4])] {
                    game.enter_level(dim).unwrap();
                    game.world
                        .chunks
                        .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
                    for conn in conns {
                        let mut player = guest(conn);
                        player.dimension = dim.id().into();
                        player.loaded = true;
                        let spec = conn == 3;
                        let x = if spec { 15 } else { 5 + conn as i32 };
                        player.position = [f64::from(x) + 0.5, 64.0, 8.5];
                        if spec {
                            player.game_mode = SPECTATOR;
                            assert!(!game.guest_body_ticks(&player));
                        }
                        let pos = [x, 63, 8];
                        game.world.set(pos[0], pos[1], pos[2], stone).unwrap();
                        let host = game.lan.as_mut().unwrap();
                        host.hub.guests.push(player);
                        let play = host.play.entry(conn).or_default();
                        if spec {
                            play.destroy_delay = 5;
                        } else {
                            play.digging = Some((pos, [0, 1, 0]));
                            play.mining = Some(Mining {
                                pos,
                                progress: 0.0,
                                per_tick: 0.0,
                                ticks: 1,
                                crack: None,
                            });
                            play.last_progress = -1;
                        }
                        game.as_guest(conn, |game| {
                            let health = &mut game.player.as_mut().unwrap().health;
                            health.invulnerable_time = 8;
                            health.hurt.cooldown = 5;
                        })
                        .unwrap();
                    }
                }
                game.enter_level(Dim::Overworld).unwrap();
                game.tick_server(false).unwrap();

                let host = game.lan.as_ref().unwrap();
                for conn in [1, 2] {
                    let mining = host.play[&conn].mining.as_ref().unwrap();
                    assert_eq!(mining.ticks, 2);
                    assert!(mining.progress > 0.0);
                }
                assert_eq!(host.play[&4].mining.as_ref().unwrap().ticks, 1);
                assert_eq!(host.play[&3].destroy_delay, 4);
                use Event::*;
                assert_eq!(
                    *heard.lock().unwrap(),
                    [
                        EntityPre(20),
                        EntityPre(22),
                        Dig(123, 0),
                        EntityPost(20),
                        EntityPost(22),
                        EntityPre(21),
                        EntityPre(23),
                        Dig(124, 0),
                        EntityPost(21),
                        PlayerPre(123, "minecraft:overworld", Some(7), Some(4)),
                        PlayerPost(123),
                        PlayerPre(124, "minecraft:the_nether", Some(7), Some(4)),
                        PlayerPost(124),
                        PlayerPre(126, "minecraft:the_nether", Some(7), Some(5)),
                        PlayerPost(126),
                    ],
                );
            })
        },
    );
}
