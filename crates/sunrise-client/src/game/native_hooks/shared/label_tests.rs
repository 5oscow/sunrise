use super::*;

#[test]
#[ignore = "daily"]
fn inventory_label_default_follows_image_height_not_inventory_slots() {
    mod_platform::testing::with_mods(mod_api::Mods::default(), || {
        crate::game::native_entity_refs::tests::with_client_game(|game| {
            game.screen = Screen::Inventory;
            game.container = Some((1, 2, 3));
            game.mod_screen = Some(OpenNativeScreen {
                layout_revision: 0,
                slot_moves: Default::default(),
                key: (1, 2, 3),
                window: 1,
                block: "test:block".into(),
                screen: mod_api::Screen {
                    width: 176.0,
                    own_background: true,
                    ..Default::default()
                },
                data: Vec::new(),
                extra: None,
            });
            let (ox, oy) = (17.0, 23.0);
            for (height, inventory_y, label_y) in [
                (166.0, 84.0, 72.0), // Standard container.
                (133.0, 51.0, 39.0), // Hopper.
                (168.0, 85.0, 74.0), // Three-row chest.
                (167.0, 84.0, 73.0), // Shulker box.
                (166.0, 30.0, 72.0), // Moving slots does not move the label.
            ] {
                let screen = &mut game.mod_screen.as_mut().unwrap().screen;
                screen.height = height;
                screen.inventory_y = inventory_y;
                let mut actual = HudBatch::scratch([512, 512]);
                assert!(game.draw_native_screen(&mut actual, ox, oy));
                let mut expected = HudBatch::scratch([512, 512]);
                expected.text(
                    &game.gui.atlas,
                    &game.translate_with_fallback("container.inventory", "Inventory"),
                    ox + 8.0,
                    oy + label_y,
                    rgba(64, 64, 64, 255),
                    false,
                );
                assert!(!expected.vertices.is_empty());
                let geometry = |batch: &HudBatch| {
                    batch
                        .vertices
                        .iter()
                        .map(|v| (v.pos, v.uv))
                        .collect::<Vec<_>>()
                };
                assert_eq!(geometry(&actual), geometry(&expected), "height {height}");
            }
        });
    });
}
