//! `ServerPlayer.doTick` runs `super.tick()` (Player.tick, where NeoForge
//! posts `PlayerTickEvent.Pre`/`Post`) only `if (!isSpectator() ||
//! !touchingUnloadedChunk())` (#4520).
use super::*;
use crate::game::native_entity_refs::tests::with_game;
use crate::game::native_item_places::tests::{guest, open_to_lan};
use horizon_client::{chunk::Chunk, effects::EffectInstance};
use mod_api::HostSide;
use std::sync::{Arc, Mutex};

struct Log(Arc<Mutex<Vec<(&'static str, u128)>>>);
#[mod_api::__subscriptions]
impl mod_api::Hooks for Log {
    fn player_tick_pre(&self, _: HostSide<'_>, event: &mut mod_api::PlayerTickPreEvent) {
        self.0.lock().unwrap().push(("pre", event.player.uuid()));
    }
    fn player_tick_post(&self, _: HostSide<'_>, event: &mut mod_api::PlayerTickPostEvent) {
        self.0.lock().unwrap().push(("post", event.player.uuid()));
    }
}

/// A loaded guest at `x` in chunk row z = 0 with 100 ticks of luck. `guest`
/// gives connection `conn` the UUID `conn + 122`.
fn placed(conn: u32, x: f64, game_mode: i32) -> horizon_client::lan::guest::Guest {
    let mut g = guest(conn);
    g.loaded = true;
    g.position = [x, 64.0, 8.5];
    g.game_mode = game_mode;
    g.effects.add(EffectInstance::new("luck", 100, 0));
    g
}

fn luck_left(game: &Game<'_>) -> Vec<i32> {
    let host = game.lan.as_ref().unwrap();
    host.hub
        .guests
        .iter()
        .map(|g| g.effects.get("luck").unwrap().duration)
        .collect()
}

/// Each guest's ender pearl cooldown, in ticks left.
fn pearl_left(game: &Game<'_>) -> Vec<i32> {
    let host = game.lan.as_ref().unwrap();
    host.hub
        .guests
        .iter()
        .map(|g| {
            host.play[&g.conn]
                .cooldowns
                .remaining()
                .find(|(group, _)| &***group == "minecraft:ender_pearl")
                .unwrap()
                .1
        })
        .collect()
}

#[test]
#[ignore = "daily"]
fn a_spectator_touching_an_unloaded_chunk_skips_its_tick_and_its_hooks() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let mods = vec![mod_api::ModEntry::new(
        "log",
        Box::new(Log(Arc::clone(&heard))),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        with_game(|game| {
            open_to_lan(game);
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            assert!(!game.world.chunks.contains_key(&(1, 0)));
            // Box half-width 0.3, inflated by 1: x 7.2..9.8 stays in chunk 0,
            // while x 14.2..16.8 (Mth.ceil 17) reaches chunk 1.
            let survivor = placed(1, 8.5, 0);
            let watcher = placed(2, 8.5, SPECTATOR);
            let edge = placed(3, 15.5, SPECTATOR);
            // A survival player touching the same unloaded chunk still ticks.
            let survivor_at_edge = placed(4, 15.5, 0);
            game.lan.as_mut().unwrap().hub.guests.extend([
                survivor,
                watcher,
                edge,
                survivor_at_edge,
            ]);
            for conn in 1..=4 {
                game.lan
                    .as_mut()
                    .unwrap()
                    .play
                    .entry(conn)
                    .or_default()
                    .cooldowns
                    .add("minecraft:ender_pearl".into(), 5);
            }

            game.tick_guests();
            assert_eq!(
                std::mem::take(&mut *heard.lock().unwrap()),
                [
                    ("pre", 123),
                    ("post", 123),
                    ("pre", 124),
                    ("post", 124),
                    ("pre", 126),
                    ("post", 126)
                ],
                "the spectator at the edge (125) hears neither hook"
            );
            assert_eq!(luck_left(game), [99, 99, 100, 99], "its effects wait too");
            // Player.tick's ItemCooldowns.tick is the body's as well.
            assert_eq!(pearl_left(game), [4, 4, 5, 4]);

            // Once the chunk it touches loads, its body ticks again.
            game.world
                .chunks
                .insert((1, 0), Arc::new(Chunk::empty(1, 0)));
            game.tick_guests();
            assert_eq!(
                std::mem::take(&mut *heard.lock().unwrap()),
                [
                    ("pre", 123),
                    ("post", 123),
                    ("pre", 124),
                    ("post", 124),
                    ("pre", 125),
                    ("post", 125),
                    ("pre", 126),
                    ("post", 126)
                ]
            );
            assert_eq!(luck_left(game), [98, 98, 99, 98]);
            assert_eq!(pearl_left(game), [3, 3, 4, 3]);
        })
    });
}
