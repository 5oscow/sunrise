#!/usr/bin/env python3
"""One-shot clean break from broadcast loot hooks to registered codecs."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def transform(path, change):
    file = ROOT / path
    old = file.read_text()
    new = change(old)
    assert new != old, path
    file.write_text(new)


def function(text, name, docs=False):
    match = re.search(r"(?m)^([ \t]*)(?:pub(?:\([^)]*\))? )?fn " + name + r"\(", text)
    assert match, name
    start = match.start()
    if docs:
        lines = text[:start].splitlines(keepends=True)
        while lines and (lines[-1].lstrip().startswith("///") or lines[-1].lstrip().startswith("#[")):
            start -= len(lines.pop())
    brace = text.index("{", match.end())
    depth = 1
    end = brace + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[:start] + text[end + (text[end:end + 1] == "\n"):]


for file in ["hooks/loot_function.rs", "hooks/loot_condition.rs", "loot_callbacks.rs", "exports/loot_callbacks.rs"]:
    (ROOT / "crates/sunrise-mod-vocab/src" / file).unlink()

transform("crates/sunrise-world/src/loot.rs", lambda text: function(function(text, "native_invoke_loot_function", True), "native_invoke_loot_condition", True))
transform("crates/sunrise-world/src/native_load_hooks.rs", lambda text: re.sub(r"(?m)^    pub loot_(?:function|condition):[^\n]*\n", "", function(function(text, "loot_function"), "loot_condition")))
transform("crates/sunrise-client/src/game/modding/api_values/loot_values.rs", lambda text: function(function(text, "dispatch_loot_function"), "dispatch_loot_condition"))
transform("crates/sunrise-client/src/game/modding/api_values.rs", lambda text: text.replace("pub(crate) use loot_values::{dispatch_loot_condition, dispatch_loot_function};\n", ""))
transform("crates/sunrise-client/src/entry.rs", lambda text: re.sub(r"(?ms)^            loot_function:.*?^            },\n            loot_condition:.*?^            },\n", "", text))
for file in ["crates/sunrise-client/tests/load_defaults.rs", "crates/sunrise-client/src/game/modding/native_data_bound.rs", "crates/sunrise-world/tests/it/native_load_hooks.rs"]:
    transform(file, lambda text: re.sub(r"(?m)^\s*loot_(?:function|condition):[^\n]*\n", "", text))
transform("crates/sunrise-world/src/native_load_hook_fixtures.rs", lambda text: re.sub(r"(?ms)^                loot_function:.*?^                },\n", "", text))
transform("crates/sunrise-client/src/game/native_hooks/shared.rs", lambda text: function(text, "native_loot_kinds_stay_hookable", True))
