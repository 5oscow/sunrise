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
                assert_eq!(
                    game.native_hidden_slots("minecraft:generic_9x3", Some(pos), 27),
                    [4]
                );
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

/// What the hopper screens' parts heard, as (the part's mod, the hook).
type Heard = Arc<Mutex<Vec<(&'static str, &'static str)>>>;
/// Mod `owner`'s screen on vanilla's `minecraft:hopper` for its machine: it hides slot
/// `HIDDEN`, its button is `HIDDEN` and its tooltip line names it. Each mod's is its own
/// kind, as two mods' parts are.
#[derive(Clone)]
struct HopperScreen<const HIDDEN: usize> {
    owner: &'static str,
    heard: Heard,
}
#[mod_api::sunrise_part]
impl<const HIDDEN: usize> mod_api::MenuPart for HopperScreen<HIDDEN> {
    fn screen(
        &self,
        _: &dyn mod_api::Host,
        _: &mut mod_api::ScreenEvent<'_>,
    ) -> Option<mod_api::Screen> {
        self.heard.lock().unwrap().push((self.owner, "screen"));
        Some(mod_api::Screen {
            slots: (0..5)
                .map(|i| (i != HIDDEN).then(mod_api::ScreenSlot::default))
                .collect(),
            own_labels: true,
            own_background: true,
            ..Default::default()
        })
    }
    fn screen_draw(&self, _: &dyn mod_api::ClientHost, _: &mut mod_api::ScreenDrawEvent<'_>) {
        self.heard.lock().unwrap().push((self.owner, "draw"));
    }
    fn screen_click(
        &self,
        _: &dyn mod_api::ClientHost,
        _: &mut mod_api::ScreenClickEvent<'_>,
    ) -> Option<i32> {
        self.heard.lock().unwrap().push((self.owner, "click"));
        Some(HIDDEN as i32)
    }
    fn screen_tooltip(
        &self,
        _: &dyn mod_api::ClientHost,
        event: &mut mod_api::ScreenTooltipEvent<'_>,
    ) {
        self.heard.lock().unwrap().push((self.owner, "tooltip"));
        event.lines.push((mod_api::Text::literal(self.owner), 0));
    }
}
/// Mod `owner`: its block `<owner>:machine`, whose container opens `minecraft:hopper`,
/// with its `HopperScreen` on that type.
struct HopperMod<const HIDDEN: usize>(HopperScreen<HIDDEN>);
#[mod_api::__subscriptions]
impl<const HIDDEN: usize> mod_api::Hooks for HopperMod<HIDDEN> {
    fn register(&self, r: &mut dyn mod_api::Registrar, _: &mod_api::RegistrationContext) {
        let block = r.block_key(&format!("{}:machine", self.0.owner));
        r.block_container(
            &block,
            mod_api::ModContainer {
                slots: 5,
                menu: Some("minecraft:hopper".into()),
                ..Default::default()
            },
        );
        r.menu_part("minecraft:hopper", self.0.clone());
    }
    fn blocks(&self, _: &mod_api::RegistrationContext, event: &mut mod_api::BlocksEvent<'_>) {
        let id = format!("{}:machine", self.0.owner);
        event
            .blocks
            .push(mod_api::Block::new(&id, "minecraft:barrel").with_item(false));
    }
}
/// Two mods' machines opening vanilla's hopper menu each get only their own mod's screen:
/// its layout on both sides, its drawing, its button, its tooltip and its slot controls. A
/// vanilla hopper keeps vanilla's screen.
#[test]
#[ignore = "daily"]
fn two_mods_sharing_a_vanilla_menu_type_keep_their_screens_apart() {
    // Content registries are process-global.
    if crate::game::native_entity_refs::tests::run_fresh_cases(
        "game::native_hooks::screen_position_tests::two_mods_sharing_a_vanilla_menu_type_keep_their_screens_apart",
        "SUNRISE_SCREEN_POSITION_HOPPER_CHILD",
        &["hoppers"],
    ) {
        return;
    }
    let heard = Heard::default();
    let alpha = HopperScreen::<0> {
        owner: "alpha",
        heard: heard.clone(),
    };
    let beta = HopperScreen::<4> {
        owner: "beta",
        heard: heard.clone(),
    };
    let mods: mod_api::Mods = vec![
        mod_api::ModEntry::new("alpha", Box::new(HopperMod(alpha))),
        mod_api::ModEntry::new("beta", Box::new(HopperMod(beta))),
    ]
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
                game.world
                    .chunks
                    .insert((0, 0), Arc::new(sunrise_client::chunk::Chunk::empty(0, 0)));
                for (x, block, hidden, owner) in [
                    (1, "alpha:machine", 0, "alpha"),
                    (2, "beta:machine", 4, "beta"),
                ] {
                    let pos = (x, 64, 0);
                    let state = game.pack.assets.registry.default_state(block).unwrap();
                    game.world.set(pos.0, pos.1, pos.2, state).unwrap();
                    heard.lock().unwrap().clear();
                    game.open_native_screen(pos, Some(pos), x, "minecraft:hopper", None);
                    game.screen = Screen::Inventory;
                    game.container = Some(pos);
                    let open = game.mod_screen.as_ref().expect("the machine's screen");
                    let (menu, block) = (open.menu.clone(), open.block.clone().unwrap());
                    let shown: Vec<_> = (0..5).filter(|&i| i != hidden).collect();
                    let ids: Vec<_> = shown.iter().map(|i| 300 + i).collect();
                    assert_eq!(game.native_screen_slots(), Some(ids));
                    let block = Some(block.as_str());
                    let mut batch = HudBatch::scratch([8, 8]);
                    game.native_screen_draw(&menu, block, None, &[], &mut batch, [0.0; 2]);
                    let mut button = None;
                    game.native_screen_click(&menu, block, &[], [0.0; 2], false, &mut button);
                    assert_eq!(button, Some(hidden as i32));
                    let mut lines = Vec::new();
                    game.native_screen_tooltip(&menu, block, &[], [0.0; 2], None, &mut lines);
                    assert_eq!(lines, [(sunrise_client::text::Text::literal(owner), 0)]);
                    let hooks = ["screen", "draw", "click", "tooltip"];
                    let only_owner: Vec<_> = hooks.iter().map(|&hook| (owner, hook)).collect();
                    assert_eq!(*heard.lock().unwrap(), only_owner);
                    // The server leaves out the owner's hidden slot, and only it.
                    assert_eq!(game.native_hidden_slots(&menu, Some(pos), 5), [hidden]);
                    // Only the owner reads and moves the screen's slots.
                    for calling in ["alpha", "beta"] {
                        let _call = mod_api::content::crash::enter_native_mod(
                            mod_api::content::crash::ModCall {
                                id: calling,
                                hook: "screen_draw",
                                part: None,
                            },
                        );
                        let owns = calling == owner;
                        assert_eq!(game.screen_slot(&menu, shown[0]).is_some(), owns);
                        let moved = game.set_screen_slot(&menu, shown[0], Some([1.0, 1.0]));
                        assert_eq!(moved.is_ok(), owns);
                    }
                }
                // A vanilla hopper asks neither mod.
                let pos = (3, 64, 0);
                let state = game
                    .pack
                    .assets
                    .registry
                    .default_state("minecraft:hopper")
                    .unwrap();
                game.world.set(pos.0, pos.1, pos.2, state).unwrap();
                heard.lock().unwrap().clear();
                game.open_native_screen(pos, Some(pos), 3, "minecraft:hopper", None);
                assert!(
                    game.mod_screen.is_none(),
                    "a vanilla hopper keeps its screen"
                );
                assert!(
                    game.native_hidden_slots("minecraft:hopper", Some(pos), 5)
                        .is_empty()
                );
                assert!(heard.lock().unwrap().is_empty());
            },
        );
    });
}
