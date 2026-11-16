use super::*;

#[test]
#[ignore = "daily"]
fn empty_slot_draws_icon_but_a_filled_slot_does_not() {
    mod_platform::testing::with_mods(mod_api::Mods::default(), || {
        crate::game::native_entity_refs::tests::with_client_game(|game| {
            let icon = "gui/sprites/container/slot/shield";
            assert!(game.gui.atlas.sprites.contains_key(icon));
            game.screen = Screen::Inventory;
            game.container = Some((1, 2, 3));
            game.mod_screen = Some(OpenNativeScreen {
                layout_revision: 0,
                slot_moves: Default::default(),
                key: (1, 2, 3),
                window: 1,
                block: "test:block".into(),
                screen: mod_api::Screen {
                    slots: vec![Some(mod_api::ScreenSlot {
                        position: [8.0, 18.0],
                        no_item_icon: Some(icon.into()),
                    })],
                    ..Default::default()
                },
                data: Vec::new(),
                extra: None,
            });
            let mut actual = HudBatch::scratch([128, 128]);
            game.draw_slot_item(&mut actual, None, 300, &None, 8.0, 18.0);
            let mut expected = HudBatch::scratch([128, 128]);
            expected.sprite_scaled(&game.gui.atlas, icon, 8.0, 18.0, 16.0, 16.0, 0xFFFF_FFFF);
            assert!(!expected.vertices.is_empty());
            let geometry = |batch: &HudBatch| {
                batch
                    .vertices
                    .iter()
                    .map(|v| (v.pos, v.uv))
                    .collect::<Vec<_>>()
            };
            assert_eq!(geometry(&actual), geometry(&expected));
            let filled = Some(ItemStack {
                item: "minecraft:stone".into(),
                count: 1,
                damage: 0,
                extra: None,
            });
            let mut with_icon = HudBatch::scratch([128, 128]);
            game.draw_slot_item(&mut with_icon, None, 300, &filled, 8.0, 18.0);
            game.mod_screen.as_mut().unwrap().screen.slots[0]
                .as_mut()
                .unwrap()
                .no_item_icon = None;
            let mut without_icon = HudBatch::scratch([128, 128]);
            game.draw_slot_item(&mut without_icon, None, 300, &filled, 8.0, 18.0);
            assert_eq!(geometry(&with_icon), geometry(&without_icon));
        });
    });
}
