# Design: dynamic textures for client mods

Answer to [#1343](https://github.com/sunrise-org/sunrise/issues/1343): a client mod names an RGBA image, uploads it,
draws it as a GUI sprite, updates it and frees it; several per mod, none reachable by another mod. The how-to is
[Textures made at run time](../../modding/src/how-to/client/dynamic-textures.md).

## Vanilla 26.3

`TextureManager.register(id, new DynamicTexture(label, nativeImage))` makes a texture of its own under an id,
`DynamicTexture.upload()` sends changed pixels, `GuiGraphics.blit(id, ...)` draws it, and `TextureManager.release(id)`
frees it. NeoForge and Fabric API add nothing to it; mods use it without a mixin. A registered texture is not a
`ReloadableTexture`, so a resource reload leaves it as it is. Read from the issue and public documentation; the
decompiled 26.3 sources were not at hand when this was written.

## Contract

On `ClientRenderClientHost`, both `#[access(ClientSetup)]` and `&self`:

```rust
fn upload_dynamic_texture(&self, id: &Identifier, width: u32, height: u32, rgba: &[u8])
    -> Result<(), DynamicTextureError>;
fn release_dynamic_texture(&self, id: &Identifier) -> Result<(), DynamicTextureError>;
```

`DrawContext::sprite(id, src, dst, tint)` draws the texture, `src` in its own pixels: no new draw call. The refusals
(`NotOwned`, `BadSize`, `ModBudget`, `TotalBudget`, `Unknown`, `Unavailable`) and the budget constants
(`DYNAMIC_TEXTURE_MAX_SIDE`, `_MOD_COUNT`, `_MOD_BYTES`, `_TOTAL_BYTES`) live in `client/dynamic_texture.rs`.

## Choices

- **A texture of its own, not an atlas slot.** The HUD already draws GUI pictures (`hud/pictures.rs`, the Realms
  panorama and preset icons): a `Picture` is a run of `Blend::Picture(id)` vertices that the renderer binds its own
  texture for, made when a frame first draws it and dropped after the last frame that drew it. A dynamic texture is a
  `Picture` made from the mod's pixels, kept by `hud::dynamic_textures::DynamicTextures` on `Game`. Packing into the
  GUI atlas instead would need an allocator inside a fixed-size atlas, a repack on every reload, and would cap every
  mod by the atlas's free space; a picture needs none of that, matches vanilla (one GPU texture per registered id), and
  costs nothing when no mod uploads one. The price is one bind group change per run of a texture's quads, as for
  vanilla's `blit` of a separate texture.
- **Names.** An `Identifier` in the calling mod's namespace (`crash::current_native_mod`), as runtime levels are
  owned. The texture is found by the same string `DrawContext::sprite` already takes, so a sprite and a dynamic
  texture are drawn the same way. A dynamic texture shadows a GUI sprite of the same name while it exists, as a
  registered texture shadows the file of its id in vanilla; the lookup costs a check of an empty map when no mod holds
  one.
- **Updates.** An upload replaces the texture's `Picture` (new pixels, any size); the renderer makes the new GPU
  texture on the next draw and drops the old one with the frame. Updating a texture in place (`write_texture` into the
  same GPU texture when the size is unchanged) would save the allocation for an image that changes every frame; it is
  left out until a mod needs it.
- **Budgets.** Per mod: 256 textures and 64 MiB of pixels; in all: 256 MiB; sides up to 4096 (below every supported
  GPU's 2D limit, 16384). The count bounds GPU objects and draw runs, the bytes bound the kept pixels (kept CPU-side
  so a rebuilt renderer, after a lost device or another graphics API, can make the texture again). A refusal names
  which budget and changes nothing. The numbers are a first proposal: the old shared slot was 1024 by 1024 (4 MiB).
- **Reload.** Nothing to do: the atlas rebuild does not touch pictures, and the renderer made after it (if any) makes
  each texture again from the kept pixels when it is drawn.
- **`&self`.** As `play_local_sound`: HUD, screen and tooltip hooks get `&dyn ClientHost`, and lazy baking on first
  draw is the common pattern in Java mods. The store is a `RefCell` on `Game`, borrowed only for the call.
- **`mod_image` is removed** (clean break, one implementation per behaviour): it was one 1024 by 1024 GUI atlas slot,
  `sunrise/modimage`, shared by every mod and reserved whenever a client mod was loaded. A mod uploads a dynamic
  texture instead; the atlas no longer reserves the 4 MiB slot. Diverges from upstream: horizon-rust/horizon keeps
  `native_mod_image`.

## Left to the maintainers

- The budget numbers, and whether a mod switched off (`set_mod_enabled`) or poisoned should lose its textures.
- Whether textures should take vanilla's `blur` and `clamp` sampler flags (today: nearest, repeat).
- Whether `ClientSetup` access is right, or reload listeners (`Reloading` phase) should upload too.
- Whether `gui_sprite`, `gui_nine_slice` and `GuiDraw::Blit` should reach dynamic textures as well.
