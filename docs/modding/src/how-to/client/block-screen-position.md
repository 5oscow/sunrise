# Reading the level from a block screen

`BlockEntityPart::screen_draw` version 2 exposes `event.pos: Option<[i32; 3]>`, alongside the block identifier, synchronized integer data slots and drawing origin. The position comes from the open menu's server-supplied extra data. For servers without that protocol, it is the clicked block position. A menu explicitly opened without a position reports `None`, not a fabricated origin.

Use this position with the existing both-sided level reads through the client host to inspect the client level view, for example the block beneath a machine. This view may lag the server and is not a substitute for authoritative menu data. The draw callback remains read-only and fires beneath items and labels.

Existing listeners keep the same event-based method signature and rebuild against this SDK; read the new `event.pos` field when needed.
