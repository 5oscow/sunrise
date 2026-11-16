//! A client's `LevelRef::brightness`, `EntityRef::set_sprinting` and the server's
//! `tick_rate_is_frozen`.

use super::native_entity_refs::tests::with_game;
use super::native_guest_turn_tests::with_guests;
use super::{Game, execution_role::ExecutionRole};
use mod_api::LightLayer;

/// A client reads the light of the level it shows, only while logged in.
#[test]
#[ignore = "daily"]
fn a_client_reads_the_light_of_its_level() {
    use std::sync::Arc;
    use sunrise_client::chunk::{Chunk, Light};
    use sunrise_protocol::connection::Pipe;
    with_game(|game| {
        game.options.execution_role = ExecutionRole::Client;
        game.options.dedicated = None;
        let (client, _server) = Pipe::pair();
        game.connect_local(client, "BrightnessTest").unwrap();
        game.remote.as_mut().unwrap().logged_in = true;
        let mut chunk = Chunk::empty(0, 0);
        Arc::make_mut(&mut chunk.sections[0]).light = Light::Uniform(0xC5);
        game.world.chunks.insert((0, 0), Arc::new(chunk));
        let level = game.level("minecraft:overworld").unwrap();
        let lit = [3, -60, 5];
        assert_eq!(
            game.level_brightness(level, LightLayer::Block, lit),
            Some(5)
        );
        assert_eq!(game.level_brightness(level, LightLayer::Sky, lit), Some(12));
        assert_eq!(
            game.level_brightness(level, LightLayer::Sky, [16, -60, 0]),
            None
        );
        assert_eq!(
            game.level_can_see_sky(level, lit),
            None,
            "that read stays on the server"
        );
        game.remote.as_mut().unwrap().logged_in = false;
        assert_eq!(game.level_brightness(level, LightLayer::Block, lit), None);
    });
}

/// A guest's sprinting is written to its row, so it survives the turn boundary the body is
/// rebuilt at, and its speed modifier stays with the flag; a stale handle sets nothing.
#[test]
#[ignore = "daily"]
fn a_guests_sprinting_survives_a_turn() {
    with_guests(|game: &mut Game<'_>| {
        let entity = game.player(13).unwrap().entity();
        // A guest has no body until its first turn.
        assert!(game.as_guest(13, |_| ()).is_some());
        assert_eq!(game.entity_is_sprinting(entity), Some(false));
        let stored = |game: &mut Game<'_>| {
            let lan = game.lan.as_mut().unwrap();
            let flag = lan.hub.update_guest(13, |g| g.sprinting).unwrap();
            let modifier = lan.hub.guests.motion_mut(13).unwrap().has_sprint_modifier();
            (flag, modifier)
        };
        assert!(game.entity_set_sprinting(entity, true));
        assert_eq!(stored(game), (true, true));
        assert_eq!(game.entity_is_sprinting(entity), Some(true));
        // The next turn starts from the row, and ends with the row still set.
        let mut in_turn = None;
        game.as_guest(13, |game| {
            in_turn = Some(game.entity_is_sprinting(entity));
        });
        assert_eq!(in_turn, Some(Some(true)));
        assert_eq!(stored(game), (true, true));
        // Inside a turn the write goes to the row as well.
        game.as_guest(13, |game| assert!(game.entity_set_sprinting(entity, false)));
        assert_eq!(stored(game), (false, false));
        let stale = mod_api::EntityRef::new(
            mod_api::WorldEpoch::new(entity.epoch().get() + 1),
            entity.id(),
        );
        assert!(!game.entity_set_sprinting(stale, true));
    });
}

/// The server's frozen state follows `/tick freeze`; a client has none.
#[test]
#[ignore = "daily"]
fn the_tick_rate_is_frozen_on_a_server_only() {
    with_game(|game| {
        game.options.dedicated = Some(Vec::new());
        game.clock.set_frozen(true);
        assert_eq!(game.tick_rate_is_frozen(), Some(true));
        game.clock.set_frozen(false);
        assert_eq!(game.tick_rate_is_frozen(), Some(false));
        game.options.execution_role = ExecutionRole::Client;
        game.options.dedicated = None;
        assert_eq!(game.tick_rate_is_frozen(), None);
    });
}

#[test]
#[ignore = "daily"]
fn client_player_state_reads_its_own_copy_only() {
    use super::native_client_session::tests::logged_client;
    use super::native_entity_refs::tests::with_client_game;
    use sunrise_client::{items::ItemStack, motion::PlayerMotion};
    use sunrise_core::geometry::Vec3;
    mod_platform::testing::with_mods(Default::default(), || {
        with_client_game(|game| {
            let _server = logged_client(game);
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            let local = game.local_player().unwrap();
            let entity = local.entity();
            let other = mod_api::EntityRef::new(entity.epoch(), 99);
            let other_player = mod_api::PlayerRef::new(other, 99);
            game.inventory[5] = Some(ItemStack::new("minecraft:stone", 3));
            for flag in [true, false] {
                let body = game.player.as_mut().unwrap();
                body.ground.on_ground = flag;
                body.fluids.was_touching_water = flag;
                body.swimming = flag;
                body.air = if flag { 87 } else { 12 };
                let host = &*game as &dyn mod_api::ClientHost;
                assert_eq!(entity.on_ground(host), Some(flag));
                assert_eq!(entity.is_in_water(host), Some(flag));
                assert_eq!(entity.is_swimming(host), Some(flag));
                assert_eq!(entity.air_supply(host), Some(if flag { 87 } else { 12 }));
                assert_eq!(entity.max_air_supply(host), Some(300));
                assert_eq!(other.on_ground(host), None);
                assert_eq!(other.is_in_water(host), None);
                assert_eq!(other.is_swimming(host), None);
                assert_eq!(other.air_supply(host), None);
                assert_eq!(other.max_air_supply(host), None);
                let slots = local.inventory_slots(host).unwrap();
                assert_eq!(slots.len(), 1);
                assert_eq!(slots[0].0, 5);
                assert_eq!(slots[0].1.count, 3);
                assert_eq!(other_player.inventory_slots(host), None);
            }
        });
    });
}
