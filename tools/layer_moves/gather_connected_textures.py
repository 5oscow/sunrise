#!/usr/bin/env python3
"""Gather the existing CTM/Fusion implementations under the pack provider.

Run on the parent of the move commit from the repository root. The mesh
output types and the original type paths stay put for this move.
"""

from pathlib import Path

render = Path("crates/sunrise-client-render/src")
formats = render / "pack_formats"
connected_path = render / "connected.rs"
fusion_path = render / "fusion.rs"
module_path = formats / "mod.rs"

connected = connected_path.read_text()
start = connected.index("/// A tile as the mesher draws it.")
end = connected.index("struct LoadedRule {", start)
(formats / "optifine_ctm.rs").write_text(
    connected[:start].replace(
        "use crate::resource_source::ResourceSource;",
        "use crate::connected::{Overlay, Pick, TileLayer};\n"
        "use crate::resource_source::ResourceSource;",
    )
    + connected[end:]
)
connected_path.write_text(
    "//! Resolved connected-texture output consumed by terrain meshing.\n\n"
    "pub use crate::pack_formats::optifine_ctm::ConnectedTextures;\n\n"
    + connected[start:end].rstrip()
    + "\n"
)

fusion_path.rename(formats / "fusion.rs")
fusion_path.write_text(
    "//! Fusion's resolved connecting textures.\n\n"
    "pub use crate::pack_formats::fusion::FusionTextures;\n"
)
module = module_path.read_text()
module_path.write_text(
    module.replace(
        "mod item_bars;\n",
        "pub(crate) mod fusion;\nmod item_bars;\npub(crate) mod optifine_ctm;\n",
    )
)
