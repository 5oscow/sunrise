//! `ClientRenderClientHost::upload_dynamic_texture` and `release_dynamic_texture`: a client
//! mod's run-time textures (vanilla's `DynamicTexture` in the `TextureManager`), kept for the
//! mod whose hook or part is running (`crash::current_native_mod`) and drawn by
//! `native_sprite` as GUI pictures of their own.
use super::Game;
use mod_api::{DynamicTextureError, Identifier};
use std::sync::Arc;
use sunrise_client::hud::pictures::Picture;

impl Game<'_> {
    /// The calling mod's texture `id` made, or given new pixels: `rgba` is `width * height`
    /// pixels of four bytes. It takes `&self`, so a HUD or screen hook can upload as it draws.
    /// `Unavailable` on a server, which draws nothing.
    pub(in crate::game) fn upload_dynamic_texture(
        &self,
        id: &Identifier,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), DynamicTextureError> {
        if !self.native_window() {
            return Err(DynamicTextureError::Unavailable);
        }
        let owner = mod_api::content::crash::current_native_mod();
        self.dynamic_textures
            .borrow_mut()
            .upload(owner, id, width, height, rgba)
    }
    /// The calling mod's texture `id` freed.
    pub(in crate::game) fn release_dynamic_texture(
        &self,
        id: &Identifier,
    ) -> Result<(), DynamicTextureError> {
        if !self.native_window() {
            return Err(DynamicTextureError::Unavailable);
        }
        let owner = mod_api::content::crash::current_native_mod();
        self.dynamic_textures.borrow_mut().release(owner, id)
    }
    /// The dynamic texture sprite `name` draws (`native_sprite`), when a mod uploaded one
    /// under it.
    pub(in crate::game) fn dynamic_texture(&self, name: &str) -> Option<Arc<Picture>> {
        self.dynamic_textures.borrow().get(name).cloned()
    }
}
