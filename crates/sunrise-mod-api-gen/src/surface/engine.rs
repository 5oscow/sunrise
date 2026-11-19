//! Project SDK staging's source catalogs into the surface without rewriting the tree.
use mod_surface::{EngineContract, EngineItem};
use std::path::Path;

pub(super) fn collect(
    root: &Path,
    wire: &crate::points::WireTypes,
    with_internals: bool,
) -> Result<Vec<EngineItem>, String> {
    let mut items = Vec::new();
    for point in crate::points::points_with(root, wire)? {
        // points.json also carries refusals for diagnostics, but none of these rows admits
        // an injection. Losing a point's last listener place is a removal from this surface.
        if point.api_refusal.is_none() {
            items.push(EngineItem::new(
                format!("point:{}", point.target),
                EngineContract::Point {
                    name: point.name,
                    signature_hash: point.signature_hash,
                },
            ));
        }
    }
    for published in crate::loops::collect(root)? {
        items.push(EngineItem::new(
            format!("loop:{}", published.name),
            EngineContract::Loop {
                entry: published.entry,
                generic: published.generic,
                slot: published.slot,
                side: published.side,
            },
        ));
    }
    if !with_internals {
        return Ok(items);
    }
    for internal in crate::internals::rule_catalog(root)? {
        let mut id = format!("internal:{}:{}", internal.kind, internal.path);
        if !internal.cfg.is_empty() {
            id.push_str(&format!(" [{}]", internal.cfg));
        }
        items.push(EngineItem::new(
            id,
            EngineContract::Internal {
                signature: internal.signature,
                signature_hash: internal.hash,
                shape_only: internal.shape_only,
                opaque_reason: internal.opaque_reason.map(str::to_owned),
            },
        ));
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn listed_internals_are_only_collected_when_requested() {
        let tree = tempfile::tempdir().unwrap();
        let root = tree.path();
        let write = |file: &str, text: &str| {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        write("crates/sunrise-engine-slots/Cargo.toml", "[dependencies]\n");
        write(
            "crates/sunrise-client/src/game.rs",
            "fn tick() {}\n#[cfg(unix)] fn helper() {}\n",
        );
        let wire = Default::default();
        let default = collect(root, &wire, false).unwrap();
        assert_eq!(default.len(), 1);
        assert_eq!(default[0].id, "point:sunrise_client::game::tick");

        write("crates/sunrise-client/Cargo.toml", "[dependencies]\n");
        write("crates/sunrise-client/src/entry.rs", "");
        write("crates/sunrise-mod-api-gen/internals.txt", "");
        write("crates/sunrise-mod-api-gen/crossing-foreign.json", "[]");
        let audit = collect(root, &wire, true).unwrap();
        assert!(audit.iter().any(|item| item.id == default[0].id));
        assert!(
            audit
                .iter()
                .any(|item| item.id == "internal:fn:sunrise_client::game::helper [unix]")
        );
    }
}
