use super::*;
use mod_api::{ClientPickEvent, ClientHost, CrosshairTarget, Hooks};
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
    fn client_pick(&self, host: &mut dyn ClientHost, event: &mut ClientPickEvent<'_>) {
        assert_eq!(host.crosshair_target(), event.original);
        *event.result = event.block_candidate;
    }
}
#[mod_api::__subscriptions]
impl Hooks for Last {
    fn client_pick(&self, host: &mut dyn ClientHost, event: &mut ClientPickEvent<'_>) {
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
            game.refresh_client_pick(1.0);
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
            game.refresh_client_pick(1.0);
            assert!(game.crosshair_target().is_none());
            assert!(game.pick().is_none());
            assert!(
                game.bucket_hit().is_some(),
                "the item-specific ray ignores the pick miss"
            );
            policy.lock().unwrap().override_result = Some(Some(block));
            game.refresh_client_pick(1.0);
            let air = game
                .pack
                .assets
                .registry
                .default_state("minecraft:air")
                .unwrap();
            game.world.set(2, 2, 0, air).unwrap();
            assert!(game.pick().is_none(), "stale state refuses, not a new ray");
            game.refresh_client_pick(1.0);
            assert!(
                game.crosshair_target().is_none(),
                "stale output refuses at writeback too"
            );
            game.world.set(2, 2, 0, stone).unwrap();
            for point in [[f64::NAN, 2.0, 0.5], [f64::INFINITY; 3], [100.0; 3]] {
                policy.lock().unwrap().override_result =
                    Some(Some(CrosshairTarget::Entity { entity, point }));
                game.refresh_client_pick(1.0);
                assert!(game.crosshair_target().is_none());
            }
            let entity_hit = CrosshairTarget::Entity {
                entity,
                point: [1.5, 2.6, 0.5],
            };
            policy.lock().unwrap().override_result = Some(Some(entity_hit));
            game.refresh_client_pick(1.0);
            assert_eq!(game.crosshair_target(), Some(entity_hit));
            assert!(game.entity_targeted());
            assert_eq!(game.mob_hit().map(|(i, _, _)| game.mobs[i].id), Some(31));
            game.native_lifecycle
                .begin_open(mod_api::Authority::RemoteClient);
            game.refresh_client_pick(1.0);
            assert!(
                game.crosshair_target().is_none(),
                "old level entity is rejected while its id is still present"
            );
            let entity = game.entity(31).unwrap();
            policy.lock().unwrap().override_result = Some(Some(CrosshairTarget::Entity {
                entity,
                point: [1.5, 2.6, 0.5],
            }));
            game.refresh_client_pick(1.0);
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
            game.refresh_client_pick(1.0);
            assert_eq!(game.pick().unwrap().pos, [2, 2, 0]);
            game.yaw = 90.0;
            assert_eq!(
                game.pick().unwrap().pos,
                [2, 2, 0],
                "look changes wait for the next frame"
            );
            game.refresh_client_pick(1.0);
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
                    name: "ClientPickTest".into(),
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

struct SelectedPick(Arc<Mutex<Option<CrosshairTarget>>>);
#[mod_api::__subscriptions]
impl Hooks for SelectedPick {
    fn client_pick(&self, _: &mut dyn ClientHost, event: &mut ClientPickEvent<'_>) {
        if let Some(result) = *self.0.lock().unwrap() {
            *event.result = Some(result);
        }
    }
}
struct UnchangedAttack;
#[mod_api::__subscriptions]
impl Hooks for UnchangedAttack {
    fn client_attack_target(
        &self,
        _: &mut dyn ClientHost,
        _: &mut mod_api::ClientAttackTargetEvent<'_>,
    ) -> mod_api::Flow {
        mod_api::Flow::Continue
    }
}

fn with_pick_game(
    kind: &str,
    x: f64,
    run: impl FnOnce(&mut Game<'_>, &mut sunrise_protocol::connection::Connection),
) {
    use crate::game::{
        native_client_session::tests::logged_client, native_entity_refs::tests::with_client_game,
    };
    use sunrise_client::{chunk::Chunk, motion::PlayerMotion};
    use sunrise_core::geometry::Vec3;
    use sunrise_protocol::packets::play::{self, Clientbound as C};
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
        game.previous_position = Vec3::new(0.5, 1.0, 0.5);
        game.yaw = -90.0;
        game.pitch = 0.0;
        game.entity_reach = 3.0;
        game.block_reach = 4.5;
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
            .id(kind)
            .unwrap();
        game.handle_packet(C::AddEntity(play::AddEntity {
            id: 31,
            uuid: 31,
            kind,
            position: [x, 2.0, 0.5],
            velocity: [0.0; 3],
            pitch: 0,
            yaw: 0,
            head_yaw: 0,
            data: 0,
        }))
        .unwrap();
        run(game, &mut server);
    });
}

fn ranged_stack() -> sunrise_client::items::ItemStack {
    use sunrise_core::nbt::Tag;
    let mut stack = sunrise_client::items::ItemStack::new("minecraft:stick", 1);
    stack.data_mut().set_other(
        "minecraft:attack_range",
        Some(Tag::Compound(
            [
                ("min_reach".into(), Tag::Float(0.0f32.to_bits())),
                ("max_reach".into(), Tag::Float(4.5f32.to_bits())),
                ("hitbox_margin".into(), Tag::Float(0.0f32.to_bits())),
            ]
            .into_iter()
            .collect(),
        )),
    );
    stack
}

fn sent_actions(
    game: &Game<'_>,
    server: &mut sunrise_protocol::connection::Connection,
) -> Vec<(&'static str, i32)> {
    use sunrise_protocol::packets::{Empty, play::Serverbound as S};
    game.send(S::Punch(Empty));
    let mut actions = Vec::new();
    loop {
        let (id, payload) = server.recv().unwrap();
        match S::decode(id, &payload).unwrap() {
            S::Attack(hit) => actions.push(("attack", hit.id)),
            S::Interact(hit) => actions.push(("interact", hit.id)),
            S::UseItem(_) => actions.push(("item", 0)),
            S::Punch(_) => return actions,
            _ => {}
        }
    }
}

#[test]
#[ignore = "daily"]
fn unchanged_attack_listener_preserves_the_committed_point_through_a_wall() {
    for subscribe_attack in [false, true] {
        let selected = Arc::new(Mutex::new(None));
        let mut mods = vec![mod_api::ModEntry::new(
            "pick",
            Box::new(SelectedPick(selected.clone())),
        )];
        if subscribe_attack {
            mods.push(mod_api::ModEntry::new("attack", Box::new(UnchangedAttack)));
        }
        mod_platform::testing::with_mods(mods.into(), || {
            with_pick_game("minecraft:pig", 2.5, |game, server| {
                let entity = game.entity(31).unwrap();
                let stone = game
                    .pack
                    .assets
                    .registry
                    .default_state("minecraft:stone")
                    .unwrap();
                game.world.set(1, 2, 0, stone).unwrap();
                let target = CrosshairTarget::Entity {
                    entity,
                    point: [2.25, 2.6, 0.5],
                };
                *selected.lock().unwrap() = Some(target);
                game.refresh_client_pick(1.0);
                assert_eq!(game.crosshair_target(), Some(target));
                assert!(
                    game.targeting_entity_hit(entity).is_none(),
                    "a new occlusion ray would reject it"
                );
                game.remote_start_attack();
                assert_eq!(
                    sent_actions(game, server),
                    [("attack", 31)],
                    "a no-op attack listener must not change the selected attack"
                );
            })
        });
    }
}

#[test]
#[ignore = "daily"]
fn attack_range_indicator_obeys_block_and_miss_replacements() {
    let policy = Arc::new(Mutex::new(Policy::default()));
    let mods = vec![
        mod_api::ModEntry::new("a_pick", Box::new(First)),
        mod_api::ModEntry::new("b_pick", Box::new(Last(policy.clone()))),
    ]
    .into();
    mod_platform::testing::with_mods(mods, || {
        with_pick_game("minecraft:pig", 3.5, |game, _| {
            game.inventory[0] = Some(ranged_stack());
            let stone = game
                .pack
                .assets
                .registry
                .default_state("minecraft:stone")
                .unwrap();
            game.world.set(4, 2, 0, stone).unwrap();
            game.refresh_client_pick(1.0);
            assert!(matches!(
                game.crosshair_target(),
                Some(CrosshairTarget::Block { .. })
            ));
            assert!(
                game.crosshair_living().is_none(),
                "no raw attack-range indicator behind the selected block"
            );
            policy.lock().unwrap().override_result = Some(None);
            game.refresh_client_pick(1.0);
            assert!(
                game.crosshair_living().is_none(),
                "no raw attack-range indicator on a selected miss"
            );
        })
    });
}

#[test]
#[ignore = "daily"]
fn forward_moving_pick_survives_a_noop_listener_and_extended_replacements() {
    let mut vanilla_point = None;
    for subscribe in [false, true] {
        let selected = Arc::new(Mutex::new(None));
        let mods = if subscribe {
            vec![mod_api::ModEntry::new(
                "pick",
                Box::new(SelectedPick(selected.clone())),
            )]
        } else {
            Vec::new()
        };
        mod_platform::testing::with_mods(mods.into(), || {
            with_pick_game("minecraft:pig", 5.5, |game, _| {
                game.inventory[0] = Some(ranged_stack());
                game.previous_position.x -= 1.0;
                game.refresh_client_pick(1.0);
                let Some(CrosshairTarget::Entity { entity, point }) = game.crosshair_target()
                else {
                    panic!("the moving attack-range ray reaches this pig");
                };
                assert_eq!(entity.id(), 31);
                if let Some(vanilla) = vanilla_point {
                    assert_eq!(
                        point, vanilla,
                        "a no-op listener keeps the exact original hit point"
                    );
                } else {
                    vanilla_point = Some(point);
                }
                assert!(game.pick_distance(point) > 4.5);
                if subscribe {
                    let replacement = CrosshairTarget::Entity {
                        entity,
                        point: [5.25, 2.6, 0.5],
                    };
                    *selected.lock().unwrap() = Some(replacement);
                    game.refresh_client_pick(1.0);
                    assert_eq!(
                        game.crosshair_target(),
                        Some(replacement),
                        "replacement uses the movement-extended envelope too"
                    );
                }
            })
        });
    }
}

#[test]
#[ignore = "daily"]
fn attack_range_pick_outside_interact_range_falls_through_to_item_use() {
    for kind in ["minecraft:pig", "minecraft:item_frame"] {
        let selected = Arc::new(Mutex::new(None));
        let mods = vec![mod_api::ModEntry::new(
            "pick",
            Box::new(SelectedPick(selected.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            with_pick_game(kind, 4.5, |game, server| {
                game.inventory[0] = Some(ranged_stack());
                let entity = game.entity(31).unwrap();
                let target = CrosshairTarget::Entity {
                    entity,
                    point: [4.5, 2.6, 0.5],
                };
                *selected.lock().unwrap() = Some(target);
                game.refresh_client_pick(1.0);
                assert_eq!(game.crosshair_target(), Some(target));
                game.remote_use(mod_api::InteractionAttempt::Press);
                let actions = sent_actions(game, server);
                assert!(!actions.iter().any(|(kind, _)| *kind == "interact"));
                assert!(
                    actions.iter().any(|(kind, _)| *kind == "item"),
                    "the stored entity passes to item use, never a block behind it"
                );
            })
        });
    }
}
