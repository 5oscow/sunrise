#![cfg(test)]
//! A client mod's dynamic texture (`ClientRenderClientHost::upload_dynamic_texture`, #1343):
//! uploaded as the mod whose hook runs, drawn under its id by `native_sprite` on the real HUD
//! pipeline, out of another mod's reach, and gone once released.
use mod_api::{DynamicTextureError, Identifier};
use sunrise_client::{
    crash::{ModCall, enter_native_mod},
    gfx::entity_pixel_test::render_hud,
    hud::HudBatch,
};

const SIZE: [u32; 2] = [64, 64];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

/// `f` run as mod `id`'s HUD hook.
fn as_mod<R>(id: &'static str, f: impl FnOnce() -> R) -> R {
    let _call = enter_native_mod(ModCall {
        id,
        hook: "gui_render_post",
        part: None,
    });
    f()
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    pixels[((y * SIZE[0] + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}

#[test]
#[ignore = "daily"]
fn a_mods_dynamic_texture_draws_under_its_id_until_it_releases_it() {
    crate::game::native_entity_refs::tests::with_client_game(|game| {
        let id = Identifier::parse("painter:dynamic/panel").unwrap();
        let name = "painter:dynamic/panel";
        let green = GREEN.repeat(16 * 16);
        as_mod("painter", || game.upload_dynamic_texture(&id, 16, 16, &green)).unwrap();
        assert_eq!(
            as_mod("other", || game.release_dynamic_texture(&id)),
            Err(DynamicTextureError::NotOwned)
        );
        let mut batch = HudBatch::scratch(SIZE);
        let (src, dst) = ([0.0, 0.0, 16.0, 16.0], [16.0, 16.0, 32.0, 32.0]);
        assert!(game.native_sprite(&mut batch, name, src, dst, None));
        let pixels = render_hud(&batch, &game.gui.atlas, SIZE).expect("software Vulkan draw");
        assert_eq!(pixel(&pixels, 32, 32), GREEN, "the mod's texture");
        assert_eq!(pixel(&pixels, 4, 4), BLACK, "outside the sprite");
        as_mod("painter", || game.release_dynamic_texture(&id)).unwrap();
        let mut batch = HudBatch::scratch(SIZE);
        assert!(!game.native_sprite(&mut batch, name, src, dst, None));
    });
}
