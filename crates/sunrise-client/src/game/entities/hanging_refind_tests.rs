//! After `hanging_break` returns without cancelling, the game finds the item frame or painting
//! again by id: one a listener removed neither breaks nor is removed again.
use crate::game::hook_meddler_tests::with_meddler;

#[test]
#[ignore = "daily"]
fn a_frame_the_listener_removed_does_not_break() {
    with_meddler(
        |host| {
            assert!(
                host.commands()
                    .run_command("kill @e[type=minecraft:item_frame]")
            );
        },
        |game, gate| {
            assert!(game.run_command("summon minecraft:item_frame 8 100 8 {Facing:3b}"));
            assert!(game.run_command("summon minecraft:item_frame 12 100 8 {Facing:3b}"));
            let dropped = game.dropped.len();
            gate.arm();
            assert!(!game.break_frame_by(1, None), "the frame was gone");
            assert!(gate.fired());
            assert!(game.item_frames.is_empty());
            assert_eq!(game.dropped.len(), dropped, "nothing dropped");
        },
    );
}

#[test]
#[ignore = "daily"]
fn a_painting_the_listener_removed_is_not_removed_again() {
    with_meddler(
        |host| {
            assert!(
                host.commands()
                    .run_command("kill @e[type=minecraft:painting]")
            );
        },
        |game, gate| {
            assert!(game.run_command("summon minecraft:painting 8 100 8 {facing:0b}"));
            assert!(game.run_command("summon minecraft:painting 20 100 8 {facing:0b}"));
            gate.arm();
            assert!(
                !game.break_painting_by(1, false, None),
                "the painting was gone"
            );
            assert!(gate.fired());
            assert!(game.paintings.is_empty());
        },
    );
}
