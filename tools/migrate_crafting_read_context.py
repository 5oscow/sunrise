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
SIGNATURE = re.compile(r"(fn (matches|assemble|remaining_items)\(\s*&self,)")


def update(match: re.Match[str]) -> str:
    reader = "CraftingRemainders" if match[2] == "remaining_items" else "LoopContext"
    return f"{match[1]} _context: &dyn mod_api::{reader},"


for relative in FILES:
    path = ROOT / relative
    before = path.read_text()
    after, count = SIGNATURE.subn(update, before)
    if count < 3:
        raise SystemExit(f"expected crafting callbacks in {relative}, found {count}")
    path.write_text(after)
