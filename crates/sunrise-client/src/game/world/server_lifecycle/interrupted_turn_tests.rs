//! #2704: a crash save never writes the half-done turn of a player a panic interrupted.

use crate::game::{
    execution_role::ExecutionRole, native_entity_refs::tests::run_fresh_cases,
    native_guest_turn_tests::with_guests,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

struct Warnings(Mutex<Vec<String>>);
impl log::Log for Warnings {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        self.0.lock().unwrap().push(record.args().to_string());
    }
    fn flush(&self) {}
}
static WARNINGS: Warnings = Warnings(Mutex::new(Vec::new()));

fn player_file(root: &Path, uuid: u128) -> PathBuf {
    sunrise_client::lan::playerdata::path(root, uuid)
}

fn advancements_file(root: &Path, uuid: u128) -> PathBuf {
    root.join("players").join("advancements").join(format!(
        "{}.json",
        sunrise_protocol::crypto::uuid_string(uuid)
    ))
}

/// A turn that takes the first hotbar stack out of the inventory (an item taken, not yet
/// placed) and then panics, as a mod's hook can.
fn taking_turn(game: &mut crate::game::Game<'_>) {
    game.inventory[0] = Some(sunrise_client::items::ItemStack {
        item: "minecraft:diamond".into(),
        count: 3,
        damage: 0,
        extra: None,
    });
    panic!("mod panic in a turn");
}

/// Runs the scenario: the panic goes through the real turn helpers, then the real stop path
/// saves. `nested` puts guest 13's turn inside guest 12's.
fn scenario(nested: bool) {
    with_guests(|game| {
        game.options.execution_role = ExecutionRole::DedicatedServer;
        let root = tempfile::tempdir().unwrap();
        let root = root.path().to_path_buf();
        game.world_root = root.clone();
        game.save = std::sync::Arc::new(sunrise_client::save::SaveDir::new(
            root.clone(),
            sunrise_client::save::LevelShape::OVERWORLD,
            Some(Default::default()),
        ));
        game.level.name = "interrupted turn".into();
        game.session_lock = Some(sunrise_client::save::SessionLock::acquire(&root).unwrap());
        game.world_adopted = true;
        assert!(
            !game.advancement_tree().list.is_empty(),
            "the advancement tree is loaded, so a save would write advancement files"
        );
        for conn in [12, 13] {
            game.lan
                .as_mut()
                .unwrap()
                .hub
                .update_guest(conn, |guest| {
                    guest.loaded = true;
                    guest.load_timeout = 0;
                })
                .unwrap();
            for file in [
                player_file(&root, conn.into()),
                advancements_file(&root, conn.into()),
            ] {
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(file, format!("last saved of {conn}")).unwrap();
            }
        }
        let read = |uuid: u128| {
            (
                std::fs::read(player_file(&root, uuid)).unwrap(),
                std::fs::read(advancements_file(&root, uuid)).unwrap(),
            )
        };
        let (before_12, before_13) = (read(12), read(13));

        let panic = catch_unwind(AssertUnwindSafe(|| {
            if nested {
                game.as_guest(12, |outer| outer.as_guest(13, taking_turn));
            } else {
                game.as_guest(12, taking_turn);
            }
        }))
        .expect_err("the panic still propagates out of the turn helpers");
        assert_eq!(
            crate::game::dedicated::panic_message(panic.as_ref()),
            "mod panic in a turn"
        );
        assert!(game.lan_guest_swaps.is_empty() && game.lan_acting.is_none());

        // The stop path saves; no tick has run since, so the record stands.
        game.run_server_lifecycle(true, |_, began| {
            *began = true;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            read(12),
            before_12,
            "the interrupted player keeps the last saved files"
        );
        if nested {
            assert_eq!(
                read(13),
                before_13,
                "so does the player whose turn it was inside"
            );
        } else {
            assert_ne!(
                read(13).0,
                before_13.0,
                "a player whose turn was not interrupted is saved as ever"
            );
        }
        let warnings = WARNINGS.0.lock().unwrap();
        for name in if nested {
            vec!["guest12", "guest13"]
        } else {
            vec!["guest12"]
        } {
            assert!(
                warnings
                    .iter()
                    .any(|line| line.contains("player's turn") && line.contains(name)),
                "{name} has the warning: {warnings:?}"
            );
        }
        assert!(
            !warnings
                .iter()
                .any(|line| line.contains("player's turn") && line.contains("guest13") && !nested),
            "only the interrupted player is named"
        );
    });
}

#[test]
#[ignore = "daily"]
fn a_crash_save_keeps_the_files_of_players_whose_turn_a_panic_interrupted() {
    // The logger is process-global.
    if run_fresh_cases(
        "game::server_lifecycle::interrupted_turn_tests::a_crash_save_keeps_the_files_of_players_whose_turn_a_panic_interrupted",
        "SUNRISE_INTERRUPTED_TURN_CHILD",
        &["turns"],
    ) {
        return;
    }
    log::set_logger(&WARNINGS).unwrap();
    log::set_max_level(log::LevelFilter::Warn);
    scenario(false);
    WARNINGS.0.lock().unwrap().clear();
    scenario(true);
}
