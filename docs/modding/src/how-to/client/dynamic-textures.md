# Textures made at run time

A client mod that bakes its own images (a panel, a glow, a gauge, glyphs from its own font, a map, a schematic
view) uploads each one under a name and draws it as a GUI sprite. This is vanilla's `DynamicTexture`: Java
registers one with `TextureManager.register(id, texture)`, sends its pixels with `DynamicTexture.upload()`, draws
it with `GuiGraphics.blit(id, ...)` and frees it with `TextureManager.release(id)`.

```rust
use mod_api::prelude::*;
use mod_api::{DynamicTextureError, Identifier};

/// Bake the panel once, at client setup or the first time it is drawn.
fn bake_panel(host: &dyn ClientHost) -> Result<(), DynamicTextureError> {
    let (width, height) = (64, 32);
    let mut rgba = vec![0u8; width * height * 4];
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[24, 24, 48, 230]);
    }
    let id = Identifier::parse("mymod:dynamic/panel").unwrap();
    host.upload_dynamic_texture(&id, width as u32, height as u32, &rgba)
}

fn gui_render_post(&self, _: &dyn ClientHost, event: &mut GuiRenderPostEvent<'_>) {
    // The whole image (`src` in its own pixels), stretched over 128 by 64 GUI pixels.
    event
        .batch
        .sprite("mymod:dynamic/panel", [0.0, 0.0, 64.0, 32.0], [8.0, 8.0, 128.0, 64.0], None);
}
```

- **Names.** The id is in the calling mod's namespace (`mymod:...`); the path is the mod's to choose. A mod holds
  as many textures as its budget allows, and no mod can replace or free another's: an id in another namespace is
  `NotOwned`. While a texture exists, `DrawContext::sprite` with its id draws it, before a GUI sprite of the same
  name, as a texture registered in vanilla's `TextureManager` shadows the file of its id. `gui_sprite` does not name
  it: it is not in the GUI atlas.
- **Updates.** Upload again under the same id to change the pixels, at any size. Each upload copies the pixels and
  makes the GPU a new texture when it is next drawn, so upload when the image changed, not every frame.
- **Freeing.** `release_dynamic_texture(id)` drops the pixels at once and the GPU texture after the last frame that
  drew it. A texture that is never released lasts until the game ends.
- **Resource reloads.** A texture is not in the GUI atlas, so a resource pack change, which rebuilds the atlas,
  keeps it: there is nothing to upload again.
- **Sampling.** Nearest filtering, repeated past its edges, as the GUI's other pictures.
- **Where.** Both methods take `&self`: a HUD, screen or tooltip hook can upload as it draws, from client setup on.
  On a server they answer `Unavailable`. Native and wasm mods use the same methods.

## Budgets

| Limit | Value | Refusal |
|---|---|---|
| Longest side | `DYNAMIC_TEXTURE_MAX_SIDE`, 4096 pixels | `BadSize` |
| Textures a mod holds | `DYNAMIC_TEXTURE_MOD_COUNT`, 256 | `ModBudget` |
| Pixels a mod holds | `DYNAMIC_TEXTURE_MOD_BYTES`, 64 MiB (four bytes a pixel) | `ModBudget` |
| Pixels every mod holds | `DYNAMIC_TEXTURE_TOTAL_BYTES`, 256 MiB | `TotalBudget` |

A replacement counts only what it adds to the texture it replaces, and a release gives its share back. Pixels that
are not `width * height * 4` bytes, or a side of 0, are `BadSize`. A refused call changes nothing. Match
`DynamicTextureError` non-exhaustively.
