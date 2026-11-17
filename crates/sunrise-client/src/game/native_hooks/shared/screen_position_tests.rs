use super::*;
use std::sync::{Arc, Mutex};

struct Position(Arc<Mutex<Vec<Option<[i32; 3]>>>>);
#[mod_api::sunrise_part]
impl mod_api::BlockPart for Position {}
#[mod_api::sunrise_part]
impl mod_api::BlockEntityPart for Position {
    fn screen_draw(&self, _: &dyn mod_api::ClientHost, event: &mut mod_api::ScreenDrawEvent<'_>) {
        self.0.lock().unwrap().push(event.pos);
    }
}
struct Listener(Arc<Mutex<Vec<Option<[i32; 3]>>>>);
#[mod_api::__subscriptions]
impl mod_api::Hooks for Listener {
    fn register(&self, r: &mut dyn mod_api::Registrar, _: &mod_api::RegistrationContext) {
        let kind = r
            .block_entity_type("screen")
            .blocks(&["minecraft:chest"])
            .build();
        r.block_entity_part(&kind, Position(self.0.clone()))
            .unwrap();
    }
}
#[test]
#[ignore = "daily"]
fn screen_draw_receives_open_blocks_position() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "test",
        Box::new(Listener(seen.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods.clone(), || {
        mod_platform::__private::part_tests::install_parts_fixture(&mods).unwrap();
        crate::game::native_entity_refs::tests::with_client_game(|game| {
            game.screen = Screen::Inventory;
            game.container = Some((17, 63, -9));
            game.mod_screen = Some(OpenNativeScreen {
                layout_revision: 0,
                slot_moves: Default::default(),
                key: (17, 63, -9),
                window: 1,
                block: "minecraft:chest".into(),
                screen: mod_api::Screen {
                    own_labels: true,
                    own_background: true,
                    ..Default::default()
                },
                data: Vec::new(),
                extra: None,
            });
            let mut batch = HudBatch::scratch([128, 128]);
            assert!(game.draw_native_screen(&mut batch, 0.0, 0.0));
            assert_eq!(*seen.lock().unwrap(), [Some([17, 63, -9])]);
        });
    });
}
