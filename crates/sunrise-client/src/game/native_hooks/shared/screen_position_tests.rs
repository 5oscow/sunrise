use super::*;
use std::sync::{Arc, Mutex};

/// What each `screen_draw` was told: the block whose menu it is and its position.
type Seen = Arc<Mutex<Vec<(Option<String>, Option<[i32; 3]>)>>>;
/// A mod menu type no block opens here.
const PACK: &str = "test:pack";

struct Position(Seen);
#[mod_api::sunrise_part]
impl mod_api::MenuPart for Position {
    fn screen(
        &self,
        _: &dyn mod_api::Host,
        event: &mut mod_api::ScreenEvent<'_>,
    ) -> Option<mod_api::Screen> {
        Some(mod_api::Screen {
            height: if event.block.is_some() { 166.0 } else { 133.0 },
            own_labels: true,
            own_background: true,
            ..Default::default()
        })
    }
    fn screen_draw(&self, _: &dyn mod_api::ClientHost, event: &mut mod_api::ScreenDrawEvent<'_>) {
        let block = event.block.map(str::to_owned);
        self.0.lock().unwrap().push((block, event.pos));
    }
}
struct Listener(Seen);
#[mod_api::__subscriptions]
impl mod_api::Hooks for Listener {
    fn register(&self, r: &mut dyn mod_api::Registrar, _: &mod_api::RegistrationContext) {
        r.menu_part("minecraft:generic_9x3", Position(self.0.clone()));
        r.menu_part(PACK, Position(self.0.clone()));
    }
}
fn with_listener(test: impl FnOnce(&mut Game<'_>, &Seen)) {
    let seen = Seen::default();
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "test",
        Box::new(Listener(seen.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods.clone(), || {
        mod_platform::__private::part_tests::install_parts_fixture(&mods).unwrap();
        crate::game::native_entity_refs::tests::with_client_game(|game| test(game, &seen));
    });
}
#[test]
#[ignore = "daily"]
fn screen_draw_receives_open_blocks_position() {
    with_listener(|game, seen| {
        game.screen = Screen::Inventory;
        game.container = Some((17, 63, -9));
        game.mod_screen = Some(OpenNativeScreen {
            layout_revision: 0,
            slot_moves: Default::default(),
            key: (17, 63, -9),
            window: 1,
            menu: "minecraft:generic_9x3".into(),
            block: Some("minecraft:chest".into()),
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
        let chest = Some("minecraft:chest".to_owned());
        assert_eq!(*seen.lock().unwrap(), [(chest, Some([17, 63, -9]))]);
    });
}
/// #1172: a mod menu type opened without a block (an item's use, an entity, a command) gets
/// its type's screen, and a vanilla type gets a mod's screen only for the mod block that opens it.
#[test]
#[ignore = "daily"]
fn a_mod_menu_opened_without_a_block_gets_its_types_screen() {
    sunrise_client::containers::with_mod_menu_types(
        [(PACK.into(), 9, 0, Vec::new(), None, None)],
        || {
            with_listener(|game, seen| {
                let key = (i32::MIN, 0, 0);
                game.open_native_screen(key, None, 1, PACK, None);
                let open = game.mod_screen.as_ref().expect("the menu type's screen");
                assert_eq!((open.screen.height, &open.block), (133.0, &None));
                game.screen = Screen::Inventory;
                game.container = Some(key);
                let mut batch = HudBatch::scratch([128, 128]);
                assert!(game.draw_native_screen(&mut batch, 0.0, 0.0));
                assert_eq!(*seen.lock().unwrap(), [(None, None)]);
                game.open_native_screen(key, None, 2, "minecraft:generic_9x3", None);
                assert!(
                    game.mod_screen.is_none(),
                    "a vanilla chest keeps its screen"
                );
            });
        },
    );
}
