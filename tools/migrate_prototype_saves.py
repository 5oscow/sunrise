#!/usr/bin/env python3
"""One-shot migration of persistence to the loaded item catalog.

Run from the repository root on the parent of the generated migration commit.
Handwritten corrections belong in subsequent commits.
"""
from pathlib import Path
import re

save = Path('crates/sunrise-client/src/save.rs')
text = save.read_text()
text, count = re.subn(r'SaveTables::new\(&self\.registry\)',
                     'SaveTables::new(&self.registry, self.items.clone())', text)
assert count == 3, count
text, count = re.subn(r'SaveTables::new\((&?assets)\)',
                     r'SaveTables::new(\1, Default::default())', text)
assert count == 8, count
text, count = re.subn(r'(Persistent \{\n)(\s+)(inner:)',
                     r'\1\2items: Default::default(),\n\2\3', text)
assert count == 4, count
save.write_text(text)

loading = Path('crates/sunrise-client/src/game/world/world_loading.rs')
text = loading.read_text()
text, count = re.subn(r'(Persistent \{\n)(\s+)(inner[:,])',
                     r'\1\2items: res.shared_items(),\n\2\3', text)
assert count == 8, count
loading.write_text(text)
