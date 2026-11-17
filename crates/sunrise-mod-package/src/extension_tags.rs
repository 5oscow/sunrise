//! Extension points declared beside their engine readers, discovered at build time.
/// A tag to which a package may add data in the reserved engine namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtensionTag {
    /// Tag directory registry, for example `block`.
    pub registry: &'static str,
    /// Namespaced tag identifier.
    pub id: &'static str,
    /// The reader's contract.
    pub doc: &'static str,
}
include!(concat!(env!("OUT_DIR"), "/extension_tags.rs"));
