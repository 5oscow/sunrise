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
        // A block's screen leaves slot 4 out; without a block it shows none.
        let slots = match event.block {
            Some(_) => (0..27)
                .map(|i| (i != 4).then(mod_api::ScreenSlot::default))
                .collect(),
            None => Vec::new(),
        };
        Some(mod_api::Screen {
            height: if event.block.is_some() { 166.0 } else { 133.0 },
            slots,
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
/// A mod that adds `test:crate`, a block whose container opens vanilla's
/// `minecraft:generic_9x3`, with the `Position` screen on that type.
struct CrateMod(Seen);
#[mod_api::__subscriptions]
impl mod_api::Hooks for CrateMod {
    fn register(&self, r: &mut dyn mod_api::Registrar, _: &mod_api::RegistrationContext) {
        let block = r.block_key("test:crate");
        r.block_container(
            &block,
            mod_api::ModContainer {
                slots: 27,
                menu: Some("minecraft:generic_9x3".into()),
                ..Default::default()
            },
        );
        r.menu_part("minecraft:generic_9x3", Position(self.0.clone()));
    }
    fn blocks(&self, _: &mod_api::RegistrationContext, event: &mut mod_api::BlocksEvent<'_>) {
        event
            .blocks
            .push(mod_api::Block::new("test:crate", "minecraft:barrel").with_item(false));
    }
}
/// A mod block whose container opens a vanilla menu type: the client's screen is the type's
/// part's, told the block and its position, and the server asks the same part with the block
/// and leaves the slot it hides out of the menu.
#[test]
#[ignore = "daily"]
fn a_mod_blocks_menu_of_a_vanilla_type_gets_the_types_screen_on_both_sides() {
    // Content registries are process-global.
    if crate::game::native_entity_refs::tests::run_fresh_cases(
        "game::native_hooks::screen_position_tests::a_mod_blocks_menu_of_a_vanilla_type_gets_the_types_screen_on_both_sides",
        "SUNRISE_SCREEN_POSITION_CRATE_CHILD",
        &["crate"],
    ) {
        return;
    }
    let seen = Seen::default();
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "test",
        Box::new(CrateMod(seen.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods.clone(), || {
        crate::game::api_values::install_parts(&mods).unwrap();
        mod_api::__private::mark_game_thread();
        let base = std::path::PathBuf::from(std::env::var_os("SUNRISE_TEST_RESOURCES").unwrap());
        let root = crate::game::mods::content_root(&base, &Default::default()).unwrap();
        crate::game::native_entity_refs::tests::with_game_role(
            root.prepared().to_owned(),
            crate::game::execution_role::ExecutionRole::Client,
            |game| {
                let pos = (17, 63, -9);
                game.world.chunks.insert(
                    (1, -1),
                    Arc::new(sunrise_client::chunk::Chunk::empty(1, -1)),
                );
                let state = game
                    .pack
                    .assets
                    .registry
                    .default_state("test:crate")
                    .unwrap();
                game.world.set(pos.0, pos.1, pos.2, state).unwrap();
                game.open_native_screen(pos, Some(pos), 1, "minecraft:generic_9x3", None);
                let open = game.mod_screen.as_ref().expect("the menu type's screen");
                assert_eq!(open.block.as_deref(), Some("test:crate"));
                assert_eq!(open.screen.height, 166.0);
                game.screen = Screen::Inventory;
                game.container = Some(pos);
                let mut batch = HudBatch::scratch([128, 128]);
                assert!(game.draw_native_screen(&mut batch, 0.0, 0.0));
                let block = Some("test:crate".to_owned());
                assert_eq!(*seen.lock().unwrap(), [(block, Some([17, 63, -9]))]);
                // The server's menu for the block: asked with the block, so slot 4 is not in it.
                let containers = game.native_containers();
                game.world.block_entities.set_mod_containers(containers);
                assert_eq!(game.native_hidden_slots(pos, 27), [4]);
            },
        );
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
