#!/usr/bin/env python3
"""Move the shared NBT path and compound-merge implementation below both loot engines.

Client modules re-export the same implementation. Run once from any directory.
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
source = ROOT / "crates/sunrise-client-core/src/nbt_path.rs"
target = ROOT / "crates/sunrise-world/src/nbt_path.rs"
core = ROOT / "crates/sunrise-client-core/src/lib.rs"
world = ROOT / "crates/sunrise-world/src/lib.rs"
commands = ROOT / "crates/sunrise-client/src/data_cmd.rs"
assert source.is_file() and not target.exists()
text = source.read_text()
assert "crate::snbt::to_snbt" in text
text = text.replace("crate::snbt::to_snbt", "crate::snbt_text::to_snbt")
command_text = commands.read_text()
start = command_text.index("/// `CompoundTag.merge`:")
end = command_text.index("/// `old.copy().merge(nbt)`.", start)
merge = command_text[start:end]
anchor = "/// An error carrying the translation key and its arguments."
assert text.count(anchor) == 1
text = text.replace(anchor, merge + "\n" + anchor, 1)
core_text = core.read_text()
world_text = world.read_text()
assert core_text.count("pub mod nbt_path;") == 1
assert world_text.count("pub mod loot;") == 1
# All preconditions have been checked before the first write.
target.write_text(text)
source.unlink()
core.write_text(core_text.replace("pub mod nbt_path;", "pub use sunrise_world::nbt_path;", 1))
world.write_text(world_text.replace("pub mod loot;", "pub mod loot;\npub mod nbt_path;", 1))
commands.write_text(command_text[:start] + "pub use crate::nbt_path::merge_compound;\n\n" + command_text[end:])
