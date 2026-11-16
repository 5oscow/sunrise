//! Client mods' run-time textures (`ClientRenderClientHost::upload_dynamic_texture`), vanilla's
//! `DynamicTexture` registered in the `TextureManager` under an id: each one a [`Picture`] of
//! its own, a texture bound for its draws, never a GUI atlas sprite, so a resource reload (which
//! rebuilds the atlas) keeps them. A mod reaches only the ids of its own namespace, within its
//! own budget and the total one.
use super::{TextureFlags, pictures::Picture};
use mod_api::{
    DYNAMIC_TEXTURE_MAX_SIDE, DYNAMIC_TEXTURE_MOD_BYTES, DYNAMIC_TEXTURE_MOD_COUNT,
    DYNAMIC_TEXTURE_TOTAL_BYTES, DynamicTextureError,
};
use std::{collections::HashMap, sync::Arc};
use sunrise_core::identifier::Identifier;

/// Every mod's dynamic textures by id, and what each mod's hold.
#[derive(Default)]
pub struct DynamicTextures {
    /// Each texture by its id as `DrawContext::sprite` names it (`mymod:dynamic/panel`).
    textures: HashMap<String, Arc<Picture>>,
    /// Each mod's textures, by its namespace.
    held: HashMap<String, Held>,
    /// The bytes of pixels every mod's textures hold together.
    total: usize,
    limits: Limits,
}

/// What one mod's textures hold.
#[derive(Clone, Copy, Default)]
struct Held {
    count: usize,
    bytes: usize,
}

/// The longest side, and the budgets: the API's own, smaller in tests.
struct Limits {
    side: u32,
    count: usize,
    mod_bytes: usize,
    total_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            side: DYNAMIC_TEXTURE_MAX_SIDE,
            count: DYNAMIC_TEXTURE_MOD_COUNT,
            mod_bytes: DYNAMIC_TEXTURE_MOD_BYTES,
            total_bytes: DYNAMIC_TEXTURE_TOTAL_BYTES,
        }
    }
}

impl DynamicTextures {
    /// Mod `owner` (none outside a mod's call) uploads `rgba`, `width * height` pixels of four
    /// bytes, as texture `id`: a new texture, or new pixels at any size for one it has. Nothing
    /// changes when it is refused.
    pub fn upload(
        &mut self,
        owner: Option<&str>,
        id: &Identifier,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), DynamicTextureError> {
        let owner = owned(owner, id)?;
        let bytes = image_bytes(self.limits.side, width, height, rgba.len())
            .ok_or(DynamicTextureError::BadSize)?;
        let key = id.to_string();
        let old = self
            .textures
            .get(&key)
            .map(|picture| picture_bytes(picture));
        let held = self.held.get(owner).copied().unwrap_or_default();
        let freed = old.unwrap_or(0);
        let now = Held {
            count: held.count + usize::from(old.is_none()),
            bytes: held.bytes - freed + bytes,
        };
        if now.count > self.limits.count || now.bytes > self.limits.mod_bytes {
            return Err(DynamicTextureError::ModBudget);
        }
        let total = self.total - freed + bytes;
        if total > self.limits.total_bytes {
            return Err(DynamicTextureError::TotalBudget);
        }
        let picture = Picture::new(width, height, rgba.to_vec(), TextureFlags::default());
        self.textures.insert(key, picture);
        self.held.insert(owner.to_owned(), now);
        self.total = total;
        Ok(())
    }
    /// Mod `owner` frees its texture `id`: the pixels now, the GPU texture once no frame
    /// draws it.
    pub fn release(
        &mut self,
        owner: Option<&str>,
        id: &Identifier,
    ) -> Result<(), DynamicTextureError> {
        let owner = owned(owner, id)?;
        let picture = self
            .textures
            .remove(&id.to_string())
            .ok_or(DynamicTextureError::Unknown)?;
        let bytes = picture_bytes(&picture);
        if let Some(held) = self.held.get_mut(owner) {
            held.count -= 1;
            held.bytes -= bytes;
            if held.count == 0 {
                self.held.remove(owner);
            }
        }
        self.total -= bytes;
        Ok(())
    }
    /// The texture sprite `name` draws, when a mod uploaded one under it: a lookup only
    /// while some mod holds one.
    pub fn get(&self, name: &str) -> Option<&Arc<Picture>> {
        if self.textures.is_empty() {
            return None;
        }
        self.textures.get(name)
    }
}

/// `owner`, when `id` is in its namespace.
fn owned<'a>(owner: Option<&'a str>, id: &Identifier) -> Result<&'a str, DynamicTextureError> {
    owner
        .filter(|owner| *owner == id.namespace())
        .ok_or(DynamicTextureError::NotOwned)
}

/// The bytes of a `width` x `height` RGBA image, none when a side is 0 or past `longest`, or
/// when its pixels are not `len` bytes.
fn image_bytes(longest: u32, width: u32, height: u32, len: usize) -> Option<usize> {
    let sides = 1..=longest;
    (sides.contains(&width) && sides.contains(&height))
        .then(|| width as usize * height as usize * 4)
        .filter(|bytes| *bytes == len)
}

fn picture_bytes(picture: &Picture) -> usize {
    let [width, height] = picture.size();
    width as usize * height as usize * 4
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(text: &str) -> Identifier {
        Identifier::parse(text).unwrap()
    }

    /// A mod reaches only its own namespace; a texture's pixels are replaced whole, at any
    /// size; each refusal is named and changes nothing; the budgets count what is held now.
    #[test]
    fn each_mod_holds_its_own_textures_within_its_budget_and_the_total() {
        // Sides up to 4, three textures or 48 bytes (three 2x2 images) a mod, 96 bytes in all.
        let mut textures = DynamicTextures {
            limits: Limits {
                side: 4,
                count: 3,
                mod_bytes: 48,
                total_bytes: 96,
            },
            ..Default::default()
        };
        let panel = id("alpha:dynamic/panel");
        assert_eq!(
            textures.upload(Some("alpha"), &panel, 2, 1, &[7; 8]),
            Ok(())
        );
        // Another mod can neither overwrite nor free it, nor can a call outside a mod.
        for owner in [Some("beta"), None] {
            assert_eq!(
                textures.upload(owner, &panel, 1, 1, &[0; 4]),
                Err(DynamicTextureError::NotOwned)
            );
            assert_eq!(
                textures.release(owner, &panel),
                Err(DynamicTextureError::NotOwned)
            );
        }
        // Pixels that do not make the image, and sides out of range.
        for (width, height, len) in [(2, 1, 7), (2, 1, 9), (0, 1, 0), (5, 1, 20)] {
            assert_eq!(
                textures.upload(Some("alpha"), &panel, width, height, &vec![0; len]),
                Err(DynamicTextureError::BadSize),
                "{width}x{height}, {len} bytes"
            );
        }
        let picture = textures.get("alpha:dynamic/panel").unwrap();
        assert_eq!((picture.size(), &picture.pixels[..]), ([2, 1], &[7; 8][..]));
        // A new upload replaces the pixels and the size.
        assert_eq!(
            textures.upload(Some("alpha"), &panel, 1, 2, &[9; 8]),
            Ok(())
        );
        let picture = textures.get("alpha:dynamic/panel").unwrap();
        assert_eq!((picture.size(), &picture.pixels[..]), ([1, 2], &[9; 8][..]));

        // The mod's bytes: past them is refused; a replacement counts only the difference,
        // and a release gives its bytes back.
        let square = [0; 16];
        let beta = ["beta:a", "beta:b", "beta:c"].map(id);
        assert_eq!(
            textures.upload(Some("beta"), &beta[0], 2, 2, &square),
            Ok(())
        );
        assert_eq!(
            textures.upload(Some("beta"), &beta[1], 2, 2, &square),
            Ok(())
        );
        assert_eq!(
            textures.upload(Some("beta"), &beta[2], 4, 1, &square),
            Ok(()),
            "48 bytes: the budget exactly"
        );
        assert_eq!(
            textures.upload(Some("beta"), &beta[2], 3, 2, &[0; 24]),
            Err(DynamicTextureError::ModBudget)
        );
        assert_eq!(
            textures.get("beta:c").unwrap().size(),
            [4, 1],
            "kept as it was"
        );
        assert_eq!(
            textures.upload(Some("beta"), &beta[0], 1, 1, &[0; 4]),
            Ok(())
        );
        assert_eq!(
            textures.upload(Some("beta"), &beta[2], 3, 2, &[0; 24]),
            Ok(())
        );
        // The count: a fourth texture is refused until one is released.
        let fourth = id("beta:d");
        assert_eq!(
            textures.upload(Some("beta"), &fourth, 1, 1, &[0; 4]),
            Err(DynamicTextureError::ModBudget)
        );
        assert_eq!(textures.release(Some("beta"), &beta[0]), Ok(()));
        assert_eq!(
            textures.release(Some("beta"), &beta[0]),
            Err(DynamicTextureError::Unknown)
        );
        assert!(textures.get("beta:a").is_none());
        assert_eq!(
            textures.upload(Some("beta"), &fourth, 1, 1, &[0; 4]),
            Ok(())
        );

        // The total, every mod's together: 8 (alpha) + 44 (beta) + 32 = 84 of 96 bytes.
        let gamma = id("gamma:a");
        assert_eq!(
            textures.upload(Some("gamma"), &gamma, 4, 2, &[0; 32]),
            Ok(())
        );
        let more = id("gamma:b");
        assert_eq!(
            textures.upload(Some("gamma"), &more, 2, 2, &[0; 16]),
            Err(DynamicTextureError::TotalBudget)
        );
        assert_eq!(textures.upload(Some("gamma"), &more, 2, 1, &[0; 8]), Ok(()));
    }
}
