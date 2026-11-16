#!/usr/bin/env python3
"""Require loaded prototypes at saved-stack boundaries, not live snapshots.

Run with tree-sitter and tree-sitter-rust installed, from the repository root.
Unknown production contexts deliberately use `items`, so compilation identifies
where a catalog parameter must be threaded instead of silently inventing defaults.
"""
from pathlib import Path
from tree_sitter import Language, Parser
import tree_sitter_rust

parser = Parser(Language(tree_sitter_rust.language()))
renames = {
    'item_to_nbt_with_items': 'item_to_nbt',
    'encode_components_with_items': 'encode_components',
    'canon_stack_with_items': 'canon_stack',
    'item_entity_to_nbt_with_items': 'item_entity_to_nbt',
    'block_entity_data_with_items': 'block_entity_data',
    'block_entity_encode::encode_with_items': 'block_entity_encode::encode',
}
parameters = {
    'crates/sunrise-client/src/entity_nbt.rs': {
        'mob_to_nbt', 'ominous_item_spawner_to_nbt', 'arrow_to_nbt',
        'fireball_to_nbt', 'throwable_to_nbt', 'firework_rocket_to_nbt',
        'item_frame_to_nbt', 'boat_to_nbt', 'item_cost_to_nbt',
    },
    'crates/sunrise-client/src/entity_nbt/trident.rs': {'trident_to_nbt'},
    'crates/sunrise-items/src/equipment.rs': {'put'},
    'crates/sunrise-client/src/save.rs': {'player_tag'},
}

def walk(node):
    yield node
    for child in node.children:
        yield from walk(child)

for root in ('crates/sunrise-items', 'crates/sunrise-client'):
    for path in Path(root).rglob('*.rs'):
        if 'target' in path.parts:
            continue
        text = path.read_text()
        # These consumers inspect or merge a live patch, rather than persist it.
        if '/src/game/' in str(path) or str(path).endswith('/src/crafting.rs'):
            text = text.replace('encode_components(', 'component_snapshot(')
        for old, new in renames.items():
            text = text.replace(old, new)
        if str(path).endswith('/save/block_entity_encode.rs'):
            text = text.replace('pub fn encode_with_items(', 'pub fn encode(')
        source = text.encode()
        changes = []
        for node in walk(parser.parse(source).root_node):
            if node.type == 'function_item':
                name_node = node.child_by_field_name('name')
                name = source[name_node.start_byte:name_node.end_byte].decode()
                if name in parameters.get(str(path), set()):
                    args = node.child_by_field_name('parameters')
                    if b'items: &crate::items::Items' not in source[args.start_byte:args.end_byte]:
                        changes.append((args.named_children[-1].end_byte, b', items: &crate::items::Items'))
            if node.type != 'call_expression':
                continue
            function = node.child_by_field_name('function')
            name = source[function.start_byte:function.end_byte].decode()
            short = name.split('::')[-1]
            expected = {'item_to_nbt': 1, 'encode_item': 1, 'encode_items': 1,
                        'canon_stack': 1, 'encode_components': 1,
                        'item_entity_to_nbt': 3, 'block_entity_data': 4}.get(short)
            if name.endswith('block_entity_encode::encode'):
                expected = 3
            if expected is None:
                continue
            if str(path).endswith('/save/block_entity_encode.rs') and name in ('item_to_nbt', 'encode_items'):
                continue
            args = node.child_by_field_name('arguments')
            if len(args.named_children) != expected:
                continue
            parent = node.parent
            test = 'test' in path.stem or '/tests/' in str(path)
            game_method = False
            while parent:
                if parent.type in ('function_item', 'mod_item'):
                    ident = parent.child_by_field_name('name')
                    label = source[ident.start_byte:ident.end_byte].decode() if ident else ''
                    test |= label == 'tests' or label.endswith('_tests')
                    before = parent.prev_named_sibling
                    while before and before.type == 'attribute_item':
                        attribute = source[before.start_byte:before.end_byte]
                        test |= b'cfg(test)' in attribute or b'#[test]' in attribute
                        before = before.prev_named_sibling
                if parent.type == 'impl_item':
                    receiver = parent.child_by_field_name('type')
                    game_method |= receiver is not None and source[receiver.start_byte:receiver.end_byte].startswith(b'Game')
                parent = parent.parent
            context = 'items'
            if game_method:
                context = 'self.res.items()'
            if test or '/examples/' in str(path):
                context = '&Default::default()'
                if str(path).endswith('/src/item_data.rs'):
                    context = 'prototype_items()'
            changes.append((args.named_children[-1].end_byte, f', {context}'.encode()))
        for offset, addition in sorted(changes, reverse=True):
            source = source[:offset] + addition + source[offset:]
        if source != path.read_bytes():
            path.write_bytes(source)
