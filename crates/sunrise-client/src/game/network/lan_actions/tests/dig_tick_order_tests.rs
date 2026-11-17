//! ServerPlayer.tick's game-mode work precedes doTick's Player.tick bracket.
use super::*;
use crate::game::native_entity_refs::tests::with_game;
use crate::game::native_item_places::tests::{guest, open_to_lan};
use mod_api::HostSide;
use std::sync::{Arc, Mutex};
use sunrise_client::chunk::Chunk;

#[derive(Default)]
struct Heard {
    progress: i32,
    ticks: Vec<(&'static str, i32)>,
}
struct Observe(Arc<Mutex<Heard>>);
#[mod_api::__subscriptions]
impl mod_api::Hooks for Observe {
    fn block_break_progress_update(
        &self,
        _: &mut dyn mod_api::ServerHost,
        event: &mut mod_api::BlockBreakProgressUpdateEvent<'_>,
    ) {
        let mut heard = self.0.lock().unwrap();
        heard.progress = event.progress;
        heard.ticks.push(("dig", event.progress));
    }
    fn player_tick_pre(&self, _: HostSide<'_>, _: &mut mod_api::PlayerTickPreEvent) {
        let mut heard = self.0.lock().unwrap();
        let progress = heard.progress;
        heard.ticks.push(("pre", progress));
    }
    fn player_tick_post(&self, _: HostSide<'_>, _: &mut mod_api::PlayerTickPostEvent) {
        let mut heard = self.0.lock().unwrap();
        let progress = heard.progress;
        heard.ticks.push(("post", progress));
    }
}

#[test]
#[ignore = "daily"]
fn player_tick_hooks_observe_advanced_dig_progress_and_skipped_body_still_ticks_game_mode() {
    let heard = Arc::new(Mutex::new(Heard {
        progress: -1,
        ..Default::default()
    }));
    mod_platform::testing::with_mods(
        vec![mod_api::ModEntry::new(
            "observe",
            Box::new(Observe(Arc::clone(&heard))),
        )]
        .into(),
        || {
            with_game(|game| {
                open_to_lan(game);
                game.world
                    .chunks
                    .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
                let pos = [8, 63, 8];
                let stone = game
                    .pack
                    .assets
                    .registry
                    .default_state("minecraft:stone")
                    .unwrap();
                game.world.set(pos[0], pos[1], pos[2], stone).unwrap();
                let mut miner = guest(1);
                miner.loaded = true;
                miner.position = [8.5, 64.0, 8.5];
                let mut spectator = guest(2);
                spectator.loaded = true;
                spectator.game_mode = SPECTATOR;
                spectator.position = [15.5, 64.0, 8.5];
                assert!(!game.guest_body_ticks(&spectator));
                let host = game.lan.as_mut().unwrap();
                host.hub.guests.extend([miner, spectator]);
                host.hub.ticks = 1;
                let play = host.play.entry(1).or_default();
                play.digging = Some((pos, [0, 1, 0]));
                play.mining = Some(Mining {
                    pos,
                    progress: 0.0,
                    per_tick: 0.0,
                    ticks: 1,
                    crack: None,
                });
                play.dig_start = 0;
                // START_DESTROY_BLOCK resets the last-sent crack stage.
                play.last_progress = -1;
                host.play.entry(2).or_default().destroy_delay = 5;

                game.tick_guests();

                let host = game.lan.as_ref().unwrap();
                let mining = host.play[&1].mining.as_ref().unwrap();
                assert_eq!(mining.ticks, 2);
                assert!(mining.progress > 0.0);
                assert_eq!(
                    host.play[&2].destroy_delay, 4,
                    "the unloaded spectator's game-mode clock runs without a body tick"
                );
                let heard = heard.lock().unwrap();
                assert_eq!(heard.progress, 0, "the first crack stage is published");
                assert_eq!(
                    heard.ticks,
                    [("dig", 0), ("pre", 0), ("post", 0)],
                    "both hooks see the updated crack; the skipped spectator hears neither"
                );
            })
        },
    );
}
