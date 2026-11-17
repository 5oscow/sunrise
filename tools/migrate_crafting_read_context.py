#!/usr/bin/env python3
"""Apply #2176's mechanical crafting callback signature change to existing fixtures."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
FILES = (
    "crates/sunrise-client/src/crafting/mod_kinds_tests.rs",
    "crates/sunrise-client/src/game/modding/crafting_recipe_kind_tests.rs",
    "crates/sunrise-client/src/game/modding/rectangular_recipe_remainder_tests.rs",
    "crates/sunrise-mod-wasm/wire-loopback/tests/wire_loopback/crafting_recipe_kinds.rs",
)
SIGNATURE = re.compile(r"(fn (matches|assemble|remaining_items)\(\s*&self,)(?!\s*_context:)")


def update(match: re.Match[str]) -> str:
    reader = "CraftingRemainders" if match[2] == "remaining_items" else "LoopContext"
    return f"{match[1]} _context: &dyn mod_api::{reader},"


for relative in FILES:
    path = ROOT / relative
    before = path.read_text()
    after, count = SIGNATURE.subn(update, before)
    if count < 3 and "_context: &dyn mod_api::" not in before:
        raise SystemExit(f"expected crafting callbacks in {relative}, found {count}")
    path.write_text(after)

# Only remainder entry points need a new argument: lookups already carry their context.
CALLS = {
    "crates/sunrise-client/src/crafting.rs": [(".remainders(", ".remainders(crate::test_resources::server_loop_context(), ")],
    "crates/sunrise-client/src/mod_content_tests.rs": [(".remainders(", ".remainders(crate::test_resources::server_loop_context(), ")],
    "crates/sunrise-client/src/furnace.rs": [(".remainders(", ".remainders(crate::test_resources::server_loop_context(), ")],
    "crates/sunrise-client/examples/probe/recipe_fixture.rs": [(".remainders(", ".remainders(crate::test_resources::server_loop_context(), ")],
    "crates/sunrise-client/src/crafting/mod_kinds_tests.rs": [(".remainders(", ".remainders(context, ")],
    "crates/sunrise-client/src/menu/crafting_take.rs": [(".remainders(", ".remainders(env.loop_context, ")],
    "crates/sunrise-client/src/game/blocks/machines/crafter.rs": [(".remainders(", ".remainders(&self.loop_context, ")],
    "crates/sunrise-client/src/game/modding/native_recipes.rs": [
        (".mod_remainders_rect(", ".mod_remainders_rect(&self.loop_context, "),
        (".remainders(", ".remainders(&self.loop_context, "),
    ],
    "crates/sunrise-client/src/game/modding/native_block_states.rs": [
        ("self.recipes().remainders(", "self.recipes().remainders(&self.loop_context, "),
        ('.remainders("missing",', '.remainders(&game.loop_context, "missing",'),
    ],
}
for relative, replacements in CALLS.items():
    path = ROOT / relative
    source = path.read_text()
    for old, new in replacements:
        if old not in source:
            raise SystemExit(f"missing remainder call in {relative}: {old}")
        source = source.replace(old, new)
    path.write_text(source)
