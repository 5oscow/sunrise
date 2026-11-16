//! A client mod's run-time textures (`ClientRenderClientHost::upload_dynamic_texture` and
//! `release_dynamic_texture`, vanilla's `DynamicTexture` in the `TextureManager`): how large
//! they may be, and why one was refused.

/// The longest side of one dynamic texture, in pixels ([`DynamicTextureError::BadSize`]).
pub const DYNAMIC_TEXTURE_MAX_SIDE: u32 = 4096;
/// The most dynamic textures one mod holds at once ([`DynamicTextureError::ModBudget`]).
pub const DYNAMIC_TEXTURE_MOD_COUNT: usize = 256;
/// The most bytes of pixels (four a pixel) one mod's dynamic textures hold together, 64 MiB
/// ([`DynamicTextureError::ModBudget`]).
pub const DYNAMIC_TEXTURE_MOD_BYTES: usize = 64 << 20;
/// The most bytes of pixels every mod's dynamic textures hold together, 256 MiB
/// ([`DynamicTextureError::TotalBudget`]).
pub const DYNAMIC_TEXTURE_TOTAL_BYTES: usize = 256 << 20;

/// Why `upload_dynamic_texture` or `release_dynamic_texture` changed nothing. The mod's
/// textures, and every other mod's, are as they were before the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(sunrise_mod_api_macros::Wire)]
pub enum DynamicTextureError {
    /// The id is not in the calling mod's namespace (`mymod:dynamic/panel` for the mod
    /// `mymod`), or no mod is calling: a mod uploads and releases only its own textures.
    NotOwned,
    /// A side is 0 or longer than [`DYNAMIC_TEXTURE_MAX_SIDE`], or the pixels are not
    /// `width * height` pixels of four bytes.
    BadSize,
    /// The calling mod would hold more than [`DYNAMIC_TEXTURE_MOD_COUNT`] textures or
    /// [`DYNAMIC_TEXTURE_MOD_BYTES`] of pixels: release some first.
    ModBudget,
    /// Every mod's textures together would hold more than [`DYNAMIC_TEXTURE_TOTAL_BYTES`] of
    /// pixels.
    TotalBudget,
    /// `release_dynamic_texture` of an id with no texture.
    Unknown,
    /// Nothing draws textures here: a server's thread, a dedicated server, or no game.
    Unavailable,
}

impl std::fmt::Display for DynamicTextureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotOwned => "the id is not in the calling mod's namespace",
            Self::BadSize => "the size or the pixels do not make an image this may hold",
            Self::ModBudget => "the calling mod's dynamic textures would pass its budget",
            Self::TotalBudget => "the mods' dynamic textures would pass the total budget",
            Self::Unknown => "no dynamic texture has this id",
            Self::Unavailable => "nothing draws dynamic textures here",
        })
    }
}

impl std::error::Error for DynamicTextureError {}

impl crate::NoBackend for DynamicTextureError {
    fn no_backend() -> Self {
        Self::Unavailable
    }
}
