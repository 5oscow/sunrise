//! A native mod's general rows in the tracker: introduced on `sunrise:general_entity` to the
//! clients that read it, at the declaration's range and interval, and never to another client.
use super::*;
use mod_api::{GeneralEntityCategory, GeneralSavePolicy};
use sunrise_protocol::buf::Reader;

struct Declare;
#[mod_api::__subscriptions]
impl mod_api::Hooks for Declare {
    fn register(&self, r: &mut dyn mod_api::Registrar, _: &mod_api::RegistrationContext) {
        for (path, chunks, interval) in [("near", 2, 1), ("far", 8, 10)] {
            r.declare_general_entity(
                path,
                GeneralEntityCategory::Misc,
                0.5,
                0.5,
                GeneralSavePolicy::Save,
                true,
                chunks,
                interval,
                1,
                &[],
            )
            .unwrap();
        }
    }
}

/// `test` with `mymod:near` (2 chunks, every tick) and `mymod:far` (8 chunks, every 10 ticks)
/// declared.
fn with_types(test: impl FnOnce()) {
    mod_platform::__private::initialize().expect("test registration requires the mod API runtime");
    // A general type is declared only while the registration phase is current.
    let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::Registration);
    let declare = || vec![mod_api::ModEntry::new("mymod", Box::new(Declare))];
    mod_platform::testing::with_mods(declare().into(), || {
        mod_platform::__private::part_tests::install_parts_fixture(&declare()).unwrap();
        test()
    });
}

fn row(id: i32, kind: &str, x: f64) -> EntityState {
    EntityState {
        id,
        uuid: 0x1000 + id as u128,
        kind: kind.into(),
        position: [x, 64.0, 0.5],
        yaw: 45.0,
        pitch: -10.0,
        velocity: [0.125, 0.0, 0.0],
        ..EntityState::default()
    }
}

fn decode(bytes: &[u8]) -> play::Clientbound {
    let mut r = Reader::new(bytes);
    let id = r.varint().unwrap();
    play::Clientbound::decode(id, r.rest()).unwrap()
}

/// The general spawns in `out`, decoded.
fn spawns(out: &[crate::lan::Body]) -> Vec<general_entity::Message> {
    out.iter()
        .filter_map(|b| match decode(b) {
            play::Clientbound::CustomPayload(p) if p.channel == general_entity::CHANNEL => {
                Some(general_entity::decode(&p.data).unwrap())
            }
            _ => None,
        })
        .collect()
}

/// Whether any packet in `out` names entity `id` (an add, a move, a sync or a removal).
fn names(out: &[crate::lan::Body], id: i32) -> bool {
    out.iter().any(|b| match decode(b) {
        play::Clientbound::MoveEntityPos(play::MoveEntityPos(m))
        | play::Clientbound::MoveEntityPosRot(play::MoveEntityPosRot(m))
        | play::Clientbound::MoveEntityRot(play::MoveEntityRot(m)) => m.id == id,
        play::Clientbound::EntityPositionSync(p) => p.id == id,
        play::Clientbound::RemoveEntities(p) => p.ids.contains(&id),
        play::Clientbound::AddEntity(p) => p.id == id,
        play::Clientbound::CustomPayload(p) => p.channel == general_entity::CHANNEL,
        _ => false,
    })
}

fn viewer<'a>(
    general_entity: bool,
    chunks: &'a crate::fast_hash::FxHashSet<(i32, i32)>,
    at: [f64; 3],
    tracker: &'a mut Tracker,
    out: &'a mut Vec<crate::lan::Body>,
) -> Viewer<'a> {
    Viewer {
        entity_id: 1,
        spectator: false,
        permission: 0,
        sunrise: true,
        general_entity,
        moved: true,
        position: at,
        view_distance: 10,
        chunks,
        sources: None,
        tracker,
        out,
        gone: None,
    }
}

/// Two clients at `at`, one that reads the channel and one that doesn't, ticked over `ticks`
/// (each tick's entities from `rows`); what each was sent, tick by tick.
type Sent = Vec<Vec<crate::lan::Body>>;
fn run(at: [f64; 3], ticks: u32, rows: impl Fn(u32) -> Vec<EntityState>) -> (Sent, Sent) {
    let mut level = Entities::default();
    let (mut sunrise, mut vanilla) = (Tracker::default(), Tracker::default());
    let chunks: crate::fast_hash::FxHashSet<(i32, i32)> = (-10..=10)
        .flat_map(|x| (-10..=10).map(move |z| (x, z)))
        .collect();
    let riding = Riding::default();
    let (mut sent_sunrise, mut sent_vanilla) = (Vec::new(), Vec::new());
    for tick in 0..ticks {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        let mut viewers = [
            viewer(true, &chunks, at, &mut sunrise, &mut a),
            viewer(false, &chunks, at, &mut vanilla, &mut b),
        ];
        let states = rows(tick);
        let refs: Vec<&EntityState> = states.iter().collect();
        // As the hub's registry lookup: a general key is no entity type a client knows.
        level.tick(&refs, &riding, &mut viewers, |_| None);
        sent_sunrise.push(a);
        sent_vanilla.push(b);
    }
    assert!(!vanilla.is_tracking(7) && !vanilla.is_tracking(8));
    (sent_sunrise, sent_vanilla)
}

#[test]
fn a_reading_client_in_range_gets_one_spawn_and_another_client_nothing() {
    with_types(|| {
        let (sunrise, vanilla) = run([0.5, 64.0, 0.5], 3, |_| vec![row(7, "mymod:far", 4.5)]);
        let all: Vec<_> = sunrise.concat();
        assert_eq!(
            spawns(&all),
            [general_entity::Message::Spawn {
                id: 7,
                uuid: 0x1007,
                type_key: "mymod:far".into(),
                position: [4.5, 64.0, 0.5],
                rotation: [45.0, -10.0],
                velocity: [0.125, 0.0, 0.0],
            }]
        );
        assert!(
            !all.iter()
                .any(|b| matches!(decode(b), play::Clientbound::AddEntity(_)))
        );
        assert!(!names(&vanilla.concat(), 7));
    });
}

#[test]
fn the_declared_range_decides_who_pairs() {
    with_types(|| {
        // 40 blocks away: past `near`'s 2 chunks (32 blocks), within `far`'s 8 (128).
        let (sunrise, _) = run([40.5, 64.0, 0.5], 1, |_| {
            vec![row(7, "mymod:near", 0.5), row(8, "mymod:far", 0.5)]
        });
        let ids: Vec<i32> = spawns(&sunrise[0])
            .into_iter()
            .map(|general_entity::Message::Spawn { id, .. }| id)
            .collect();
        assert_eq!(ids, [8]);
    });
}

#[test]
fn moves_go_out_at_the_declared_interval() {
    with_types(|| {
        // Both rows move every tick; `near` updates every tick, `far` every 10.
        let moving = |tick: u32| {
            let x = 0.5 + f64::from(tick) * 0.25;
            vec![row(7, "mymod:near", x), row(8, "mymod:far", x)]
        };
        let (sunrise, _) = run([0.5, 64.0, 0.5], 21, moving);
        let moved = |id: i32| -> Vec<u32> {
            (1..21)
                .filter(|&t| {
                    sunrise[t as usize].iter().any(|b| match decode(b) {
                        play::Clientbound::MoveEntityPos(play::MoveEntityPos(m))
                        | play::Clientbound::MoveEntityPosRot(play::MoveEntityPosRot(m)) => {
                            m.id == id
                        }
                        play::Clientbound::EntityPositionSync(p) => p.id == id,
                        _ => false,
                    })
                })
                .collect()
        };
        assert_eq!(moved(7), (1..21).collect::<Vec<_>>());
        assert_eq!(moved(8).len(), 2, "{:?}", moved(8));
    });
}

#[test]
fn leaving_range_removes_the_row_and_coming_back_spawns_it_afresh() {
    with_types(|| {
        // The row walks out of `near`'s 32 blocks and back.
        let x = |tick: u32| match tick {
            0 => 0.5,
            1 => 100.5,
            _ => 1.5,
        };
        let (sunrise, _) = run([0.5, 64.0, 0.5], 3, |t| vec![row(7, "mymod:near", x(t))]);
        assert_eq!(spawns(&sunrise[0]).len(), 1);
        assert!(
            sunrise[1]
                .iter()
                .any(|b| matches!(decode(b), play::Clientbound::RemoveEntities(p) if p.ids == [7]))
        );
        assert_eq!(spawns(&sunrise[2]).len(), 1);
    });
}
