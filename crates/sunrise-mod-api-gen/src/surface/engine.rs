//! Project SDK staging's source catalogs into the surface without rewriting the tree.
use mod_surface::{EngineContract, EngineItem};
use std::path::Path;

pub(super) fn collect(
    root: &Path,
    wire: &crate::points::WireTypes,
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
