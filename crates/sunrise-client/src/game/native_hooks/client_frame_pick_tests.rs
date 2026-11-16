use super::*;
use mod_api::{ClientFramePickEvent, ClientHost, CrosshairTarget, Hooks};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Policy {
    override_result: Option<Option<CrosshairTarget>>,
    calls: usize,
}
struct First;
struct Last(Arc<Mutex<Policy>>);
#[mod_api::__subscriptions]
impl Hooks for First {
    fn client_frame_pick(&self, host: &mut dyn ClientHost, event: &mut ClientFramePickEvent<'_>) {
        assert_eq!(host.crosshair_target(), event.original);
        *event.result = event.block_candidate;
    }
}
#[mod_api::__subscriptions]
impl Hooks for Last {
    fn client_frame_pick(&self, host: &mut dyn ClientHost, event: &mut ClientFramePickEvent<'_>) {
        assert_eq!(host.crosshair_target(), event.original);
        assert_eq!(
            *event.result, event.block_candidate,
            "the second listener sees the first's write"
        );
        let mut policy = self.0.lock().unwrap();
        policy.calls += 1;
        if let Some(result) = policy.override_result {
            *event.result = result;
        }
    }
}

#[test]
#[ignore = "daily"]
fn frame_pick_is_shared_ordered_and_refuses_stale_or_invalid_output() {
    use crate::game::{
        native_client_session::tests::logged_client, native_entity_refs::tests::with_client_game,
    };
    use sunrise_client::{chunk::Chunk, motion::PlayerMotion};
    use sunrise_core::geometry::Vec3;
    use sunrise_protocol::packets::play::{self, Clientbound as C};
    let policy = Arc::new(Mutex::new(Policy::default()));
    let mods = vec![
        mod_api::ModEntry::new("a_pick", Box::new(First)),
        mod_api::ModEntry::new("b_pick", Box::new(Last(policy.clone()))),
    ]
    .into();
    mod_platform::testing::with_mods(mods, || {
        with_client_game(|game| {
            let mut server = sunrise_protocol::connection::Connection::local(logged_client(game));
            server
                .reader
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            finish_login(&mut server);
            game.native_lifecycle
                .begin_open(mod_api::Authority::RemoteClient);
            game.title = None;
            game.player = Some(PlayerMotion::new(game.motion, Vec3::new(0.5, 1.0, 0.5)).unwrap());
            game.yaw = -90.0;
            game.pitch = 0.0;
            game.block_reach = 4.5;
            game.entity_reach = 3.0;
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let kind = game
                .remote
                .as_ref()
                .unwrap()
                .builtin
                .get("minecraft:entity_type")
                .unwrap()
                .id("minecraft:pig")
                .unwrap();
            game.handle_packet(C::AddEntity(play::AddEntity {
                id: 31,
                uuid: 31,
                kind,
                position: [1.5, 2.0, 0.5],
                velocity: [0.0; 3],
                pitch: 0,
                yaw: 0,
                head_yaw: 0,
                data: 0,
            }))
            .unwrap();
            let entity = game.entity(31).unwrap();
            let stone = game
                .pack
                .assets
                .registry
                .default_state("minecraft:stone")
                .unwrap();
            game.world.set(2, 2, 0, stone).unwrap();
            game.refresh_frame_pick(1.0);
            let block = game.crosshair_target().unwrap();
            assert!(matches!(block, CrosshairTarget::Block { .. }));
            assert_eq!(
                game.pick().unwrap().pos,
                [2, 2, 0],
                "outline and interaction share this read"
            );
            assert!(!game.entity_targeted());
            assert!(
                game.mob_hit().is_none(),
                "the entity cannot steal the block interaction"
            );
            game.pick_tick_targets();
            assert_eq!(game.crosshair_target(), Some(block));
            game.release_tick_targets();
            assert_eq!(
                policy.lock().unwrap().calls,
                1,
                "input never refires the hook"
            );

            policy.lock().unwrap().override_result = Some(None);
            game.refresh_frame_pick(1.0);
            assert!(game.crosshair_target().is_none());
            assert!(game.pick().is_none());
            assert!(
                game.bucket_hit().is_some(),
                "the item-specific ray ignores the pick miss"
            );
            policy.lock().unwrap().override_result = Some(Some(block));
            game.refresh_frame_pick(1.0);
            let air = game
                .pack
                .assets
                .registry
                .default_state("minecraft:air")
                .unwrap();
            game.world.set(2, 2, 0, air).unwrap();
            assert!(game.pick().is_none(), "stale state refuses, not a new ray");
            game.refresh_frame_pick(1.0);
            assert!(
                game.crosshair_target().is_none(),
                "stale output refuses at writeback too"
            );
            game.world.set(2, 2, 0, stone).unwrap();
            for point in [[f64::NAN, 2.0, 0.5], [f64::INFINITY; 3], [100.0; 3]] {
                policy.lock().unwrap().override_result =
                    Some(Some(CrosshairTarget::Entity { entity, point }));
                game.refresh_frame_pick(1.0);
                assert!(game.crosshair_target().is_none());
            }
            let entity_hit = CrosshairTarget::Entity {
                entity,
                point: [1.5, 2.6, 0.5],
            };
            policy.lock().unwrap().override_result = Some(Some(entity_hit));
            game.refresh_frame_pick(1.0);
            assert_eq!(game.crosshair_target(), Some(entity_hit));
            assert!(game.entity_targeted());
            assert_eq!(game.mob_hit().map(|(i, _, _)| game.mobs[i].id), Some(31));
            game.native_lifecycle
                .begin_open(mod_api::Authority::RemoteClient);
            game.refresh_frame_pick(1.0);
            assert!(
                game.crosshair_target().is_none(),
                "old level entity is rejected while its id is still present"
            );
            let entity = game.entity(31).unwrap();
            policy.lock().unwrap().override_result = Some(Some(CrosshairTarget::Entity {
                entity,
                point: [1.5, 2.6, 0.5],
            }));
            game.refresh_frame_pick(1.0);
            assert!(game.entity_targeted());
            game.handle_packet(C::RemoveEntities(play::RemoveEntities { ids: vec![31] }))
                .unwrap();
            assert!(game.crosshair_target().is_none(), "removed entity refuses");
        })
    });
}

#[test]
#[ignore = "daily"]
fn frame_pick_without_listeners_keeps_the_vanilla_result() {
    use crate::game::{
        native_client_session::tests::logged_client, native_entity_refs::tests::with_client_game,
    };
    use sunrise_client::{chunk::Chunk, motion::PlayerMotion};
    use sunrise_core::geometry::Vec3;
    mod_platform::testing::with_mods(Vec::new().into(), || {
        with_client_game(|game| {
            let mut server = sunrise_protocol::connection::Connection::local(logged_client(game));
            finish_login(&mut server);
            game.native_lifecycle
                .begin_open(mod_api::Authority::RemoteClient);
            game.title = None;
            game.player = Some(PlayerMotion::new(game.motion, Vec3::new(0.5, 1.0, 0.5)).unwrap());
            game.yaw = -90.0;
            game.pitch = 0.0;
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let stone = game
                .pack
                .assets
                .registry
                .default_state("minecraft:stone")
                .unwrap();
            game.world.set(2, 2, 0, stone).unwrap();
            game.refresh_frame_pick(1.0);
            assert_eq!(game.pick().unwrap().pos, [2, 2, 0]);
            game.yaw = 90.0;
            assert_eq!(
                game.pick().unwrap().pos,
                [2, 2, 0],
                "look changes wait for the next frame"
            );
            game.refresh_frame_pick(1.0);
            assert!(game.pick().is_none());
        })
    });
}

fn finish_login(server: &mut sunrise_protocol::connection::Connection) {
    use sunrise_protocol::packets::{Empty, configuration as cfg, login};
    server.recv().unwrap();
    server.recv().unwrap();
    server
        .send(
            &login::Clientbound::LoginFinished(login::LoginFinished {
                profile: login::GameProfile {
                    uuid: 42,
                    name: "FramePickTest".into(),
                    properties: vec![],
                },
                session: 1,
            })
            .encode(),
        )
        .unwrap();
    server.recv().unwrap();
    server.recv().unwrap();
    server.recv().unwrap();
    server
        .send(&cfg::Clientbound::FinishConfiguration(Empty).encode())
        .unwrap();
    server.recv().unwrap();
}
