#!/usr/bin/env python3
"""Moves the keyboard-focus half of game/interface/pad_menu.rs (from the
"the keyboard: vanilla's focus navigation" marker to the "text fields" marker)
into the child module pad_menu/keyboard.rs. Pure move; visibility fixes follow by hand."""
import pathlib

base = pathlib.Path("crates/sunrise-client/src/game/interface")
src = base / "pad_menu.rs"
lines = src.read_text().split("\n")
start = next(i for i, l in enumerate(lines) if "// ---- the keyboard: vanilla's focus navigation" in l)
end = next(i for i, l in enumerate(lines) if l.strip() == "// ---- text fields ----")
body = lines[start + 1:end]
while body and body[-1] == "":
    body.pop()
while body and body[0] == "":
    body.pop(0)
head = '''//! The keyboard drives a menu's focus: vanilla's focus navigation (`key_focus`) applied in
//! the game, sharing the controller's pointer, focus and targets (`pad_menu`).
use super::*;

impl Game<'_> {
'''
(base / "pad_menu").mkdir(exist_ok=True)
(base / "pad_menu" / "keyboard.rs").write_text(head + "\n".join(body) + "\n}\n")
new = lines[:start] + lines[end:]
src.write_text("\n".join(new))
