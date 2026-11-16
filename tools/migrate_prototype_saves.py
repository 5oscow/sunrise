#!/usr/bin/env python3
"""One-shot migration: pass the loaded catalog into save directories.

Requires tree-sitter and tree-sitter-rust. Run from the repository root on the
parent of the generated migration commit; handwritten corrections follow it.
"""
from pathlib import Path
from tree_sitter import Language, Parser
import tree_sitter_rust

parser = Parser(Language(tree_sitter_rust.language()))

def walk(node):
    yield node
    for child in node.children:
        yield from walk(child)

for path in Path('crates/sunrise-client').rglob('*.rs'):
    if 'target' in path.parts:
        continue
    source = path.read_bytes()
    if b'SaveDir::new' not in source:
        continue
    changes = []
    for node in walk(parser.parse(source).root_node):
        if node.type != 'call_expression':
            continue
        function = node.child_by_field_name('function')
        if not source[function.start_byte:function.end_byte].decode().endswith('SaveDir::new'):
            continue
        arguments = node.child_by_field_name('arguments')
        assert len(arguments.named_children) == 2, (path, node.start_point)
        ancestors = []
        parent = node.parent
        while parent:
            if parent.type == 'function_item':
                name = parent.child_by_field_name('name')
                ancestors.append(source[name.start_byte:name.end_byte].decode())
            parent = parent.parent
        context = 'Some(Default::default())'  # Synthetic tests have no item prototypes.
        relative = str(path.relative_to('crates/sunrise-client'))
        if relative == 'src/game/world/world_loading.rs' and ancestors[-1] == 'make_streaming':
            context = 'Some(res.shared_items())'
        elif relative == 'src/game/network/remote/mod.rs' and ancestors[-1] in ('connect_remote_session', 'init_remote_for_replay'):
            context = 'None'  # These unused remote save directories never persist chunks.
        elif relative == 'src/game/world/levels.rs' and ancestors[-1] == 'open_saved_portal_levels':
            context = 'None'  # Reads portal metadata only.
        elif relative == 'src/game.rs' and ancestors[-1] == 'new':
            context = 'None'  # The title screen has no world catalog yet.
        last = arguments.named_children[-1]
        changes.append((last.end_byte, f', {context}'.encode()))
    for offset, addition in reversed(changes):
        source = source[:offset] + addition + source[offset:]
    path.write_bytes(source)
