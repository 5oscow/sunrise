//! Immutable point declarations installed by the canonical game/SDK record owner.
use mod_vocab::__private::points::Table;
use std::sync::OnceLock;

/// One checked place in the game's complete native/wasm catalog.
pub struct ApiPlace {
    /// Global catalog index.
    pub index: usize,
    /// Target function named by a manifest.
    pub target: &'static str,
    /// Whether this is the function's return place rather than its head.
    pub at_return: bool,
    /// Return type a replacement at this place cannot produce.
    pub replace_refusal: Option<&'static str>,
}

/// Immutable game-owned declarations; listener policy stays in the existing registry.
pub struct Catalog {
    /// The one native/wasm registration and activity table.
    pub table: &'static Table,
    /// Layout checked before accepting a guest's subscriptions.
    pub layout: u64,
    /// Every eligible API place, in global catalog order.
    pub places: &'static [ApiPlace],
    /// Targets with no API record and their existing refusal reasons.
    pub refusals: &'static [(&'static str, &'static str)],
}

static CATALOG: OnceLock<&'static Catalog> = OnceLock::new();

/// No canonical game catalog was installed before a point operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatalogUnavailable;

impl std::fmt::Display for CatalogUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .write_str("the game's point catalog must be installed before loading point listeners")
    }
}
impl std::error::Error for CatalogUnavailable {}

/// Install the actual game catalog once, before listener registration.
pub fn install_catalog(catalog: &'static Catalog) -> Result<(), crate::Refusal> {
    CATALOG
        .set(catalog)
        .map_err(|_| crate::Refusal::AlreadyInstalled)
}

/// Obtain the installed catalog; an unavailable provider is an error, never an empty catalog.
pub fn catalog() -> Result<&'static Catalog, CatalogUnavailable> {
    CATALOG.get().copied().ok_or(CatalogUnavailable)
}
