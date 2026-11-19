//! `/tick freeze` stops a client's level, not its HUD or its player: `Gui.tick` runs
//! `Hud.updateVignetteBrightness` on every tick the game isn't paused, and the local player
//! still ticks (`TickRateManager.isEntityFrozen` is false for a player).
use crate::game::execution_role::ExecutionRole;
use crate::game::native_entity_refs::tests::with_game;
use sunrise_protocol::connection::Pipe;

#[test]
#[ignore = "daily"]
fn a_frozen_level_still_eases_the_vignette() {
    with_game(|game| {
        // The player, in an empty chunk.
        let at = sunrise_core::geometry::Vec3::new(8.5, 70.0, 8.5);
        game.player = Some(sunrise_client::motion::PlayerMotion::new(game.motion, at).unwrap());
        game.streaming
            .insert_remote(&mut game.world, sunrise_client::chunk::Chunk::empty(0, 0));
        let eye = game.eye_position().expect("a player");
        // updateVignetteBrightness: the vignette moves a hundredth of the way to
        // 1 - the light's brightness at the eye, here from 0.5.
        let target = (1.0 - game.eye_brightness(eye)).clamp(0.0, 1.0);
        assert_ne!(target, 0.5, "a target the vignette moves to");
        // A local world's level tick eases it, as before.
        game.screen_fx.vignette_brightness = 0.5;
        game.tick_particles();
        assert_eq!(
            game.screen_fx.vignette_brightness,
            0.5 + (target - 0.5) * 0.01
        );
        // A client of a server.
        game.options.execution_role = ExecutionRole::Client;
        let (client, _server) = Pipe::pair();
        game.connect_local(client, "frozen-client").unwrap();
        let remote = game.remote.as_mut().unwrap();
        remote.logged_in = true;
        remote.loaded_sent = true;
        remote.entity_id = 42;
        // The player, placed again after joining.
        game.player = Some(sunrise_client::motion::PlayerMotion::new(game.motion, at).unwrap());
        game.streaming
            .insert_remote(&mut game.world, sunrise_client::chunk::Chunk::empty(0, 0));
        assert_eq!(game.eye_position(), Some(eye));
        game.screen_fx.vignette_brightness = 0.5;
        // A frozen level (`running` false), not `SUNRISE_STILL`.
        game.tick_remote_player(false, false).unwrap();
        let eased = 0.5 + (target - 0.5) * 0.01;
        assert_eq!(game.screen_fx.vignette_brightness, eased);
        // Under SUNRISE_STILL's hold, it waits with the rest of the HUD.
        game.tick_remote_player(false, true).unwrap();
        assert_eq!(game.screen_fx.vignette_brightness, eased);
        // A running level eases it once a tick, not twice.
        game.tick_remote_player(true, false).unwrap();
        assert_eq!(
            game.screen_fx.vignette_brightness,
            eased + (target - eased) * 0.01
        );
    });
}
