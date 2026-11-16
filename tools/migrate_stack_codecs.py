#!/usr/bin/env python3
"""Require loaded prototypes at saved-stack boundaries, not live snapshots.

Run with tree-sitter and tree-sitter-rust installed, from the repository root.
Unknown production contexts deliberately use `items`, so compilation identifies
where a catalog parameter must be threaded instead of silently inventing defaults.
"""
from pathlib import Path
import json
import sys
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
        'item_frame_to_nbt', 'boat_to_nbt', 'armor_stand_to_nbt', 'put_offers', 'offers_to_nbt',
    },
    'crates/sunrise-client/src/entity_nbt/trident.rs': {'trident_to_nbt'},
    'crates/sunrise-items/src/equipment.rs': {'put'},
    'crates/sunrise-client/src/save.rs': {'player_tag'},
}

# Overloaded engine methods need the compiler's resolved argument types.
definitions_only = {
    'crates/sunrise-client/src/game.rs': {'write_level_for_role'},
    'crates/sunrise-client/src/game/entities/boats/mod.rs': {'to_nbt'},
    'crates/sunrise-client/src/game/entities/fireworks.rs': {'to_nbt'},
    'crates/sunrise-client/src/game/entities/minecarts.rs': {'to_nbt'},
    'crates/sunrise-client/src/game/items/trident.rs': {'to_nbt'},
    'crates/sunrise-client/src/game/blocks/spill.rs': {'record', 'record_from'},
    'crates/sunrise-client/src/game/blocks/decorated_pot.rs': {'unpack_loot'},
    'crates/sunrise-client/src/game/modding/native_block_entity_refs.rs': {
        'save_with_metadata', 'save_without_metadata', 'nameable',
    },
}
resolved_calls = {}
if len(sys.argv) == 3 and sys.argv[1] == '--diagnostics':
    for line in Path(sys.argv[2]).read_text().splitlines():
        event = json.loads(line)
        if event.get('reason') != 'compiler-message':
            continue
        message = event['message']
        if not message.get('code') or message['code']['code'] != 'E0061':
            continue
        for child in message['children']:
            for span in child['spans']:
                if '/* &Items */' not in (span['suggested_replacement'] or ''):
                    continue
                parts = [part['text'] for part in span['text']]
                parts[-1] = parts[-1][:span['column_end'] - 1]
                parts[0] = parts[0][span['column_start'] - 1:]
                resolved_calls[(Path(span['file_name']).resolve(), span['byte_start'])] = '\n'.join(parts).encode()

def walk(node):
    yield node
    for child in node.children:
        yield from walk(child)

def sites(source, root, offset=0, owner=None):
    """Parse macro expressions as tuples too; token trees hide ordinary calls."""
    for node in walk(root):
        if node.type == 'macro_invocation':
            tokens = next((child for child in node.named_children if child.type == 'token_tree'), None)
            if tokens is not None:
                prefix = b'fn __migration_macro() { let _ = ('
                body = source[tokens.start_byte + 1:tokens.end_byte - 1]
                wrapped = prefix + body + b'); }'
                yield from sites(wrapped, parser.parse(wrapped).root_node,
                                 offset + tokens.start_byte + 1 - len(prefix), owner or node)
        elif node.type == 'call_expression' or (node.type == 'function_item' and owner is None):
            yield node, source, offset, owner or node

function_contexts = {}
for filename, names in parameters.items():
    content = Path(filename).read_bytes()
    for node in walk(parser.parse(content).root_node):
        if node.type != 'function_item':
            continue
        ident = node.child_by_field_name('name')
        name = content[ident.start_byte:ident.end_byte].decode()
        if name not in names or name == 'put':
            continue
        args = node.child_by_field_name('parameters')
        has_catalog = b'&crate::items::Items' in content[args.start_byte:args.end_byte]
        new_count = len(args.named_children) + int(not has_catalog)
        function_contexts[name] = new_count - 1

for root in ('crates/sunrise-items', 'crates/sunrise-client', 'crates/sunrise-client-render'):
    for path in Path(root).rglob('*.rs'):
        if 'target' in path.parts:
            continue
        text = path.read_text()
        # These consumers inspect or merge a live patch, rather than persist it.
        if '/src/game/' in str(path) or str(path).endswith(('/src/crafting.rs', '/src/cit.rs')):
            text = text.replace('encode_components(', 'component_snapshot(')
        for old, new in renames.items():
            text = text.replace(old, new)
        if str(path).endswith('/save/block_entity_encode.rs'):
            text = text.replace('pub fn encode_with_items(', 'pub fn encode(')
        if str(path).endswith(('/src/entity_nbt.rs', '/src/mobs/villager.rs')):
            text = text.replace('.map(crate::save::item_to_nbt)', '.map(|stack| crate::save::item_to_nbt(stack, items))')
            text = text.replace('.map(item_to_nbt)', '.map(|stack| item_to_nbt(stack, items))')
        if str(path).endswith('/game/entities/armor_stands.rs'):
            text = text.replace('.map(sunrise_items::item_nbt::item_to_nbt)', '.map(sunrise_core::nbt::Tag::from)')
        source = text.encode()
        changes = []
        for node, content, offset, origin in sites(source, parser.parse(source).root_node):
            if node.type == 'function_item':
                name_node = node.child_by_field_name('name')
                name = content[name_node.start_byte:name_node.end_byte].decode()
                if name in (parameters.get(str(path), set()) | definitions_only.get(str(path), set())):
                    args = node.child_by_field_name('parameters')
                    if b'&crate::items::Items' not in content[args.start_byte:args.end_byte]:
                        changes.append((offset + args.named_children[-1].end_byte, b', items: &crate::items::Items'))
            if node.type != 'call_expression':
                continue
            function = node.child_by_field_name('function')
            name = content[function.start_byte:function.end_byte].decode()
            short = name.split('::')[-1]
            expected = {'item_to_nbt': 1, 'encode_item': 1, 'encode_items': 1,
                        'canon_stack': 1, 'encode_components': 1,
                        'item_entity_to_nbt': 3, 'block_entity_data': 4}.get(short)
            method_name = short.split('.')[-1]
            if method_name in function_contexts:
                expected = function_contexts[method_name] - int(function.type == 'field_expression')
            if name.endswith('block_entity_encode::encode'):
                expected = 3
            if name.split('::')[-2:] == ['villager', 'write_nbt'] or (str(path).endswith('/src/mobs/villager.rs') and name == 'write_nbt'):
                expected = 2
            segments = name.split('::')
            if (len(segments) > 1 and segments[-2] == 'playerdata') or (str(path).endswith('/src/lan/playerdata.rs') and name in ('save', 'save_now', 'save_owner', 'migrate_owner')):
                expected = {'save': 2, 'save_now': 2, 'save_owner': 3, 'migrate_owner': 4}.get(short, expected)
            if name.endswith('.write_player'):
                expected = 1
            if (name.endswith('.enqueue') or name.endswith('.save_after')) and ('player_autosaves' in name or str(path).endswith('/src/lan/playerdata.rs')):
                expected = 2
            menu_drop = '/src/menu' in str(path) and name.endswith('.drop_stack')
            if menu_drop:
                expected = 1
            if name.endswith('.write') and not name.endswith('equipment.write') and (name.endswith('level.write') or str(path).endswith('/src/save.rs') or str(path).endswith('/src/lan/playerdata.rs')):
                expected = 1
            if str(path).endswith('/src/lan/chunks.rs') and name == 'entity_tag':
                expected = 2
            arc_catalog = len(segments) > 1 and segments[-2] == 'ChunkEncoder' and short in ('new', 'with_signs', 'with_types')
            if arc_catalog:
                expected = 5 if short == 'with_types' else 4
            args = node.child_by_field_name('arguments')
            resolved = resolved_calls.get((path.resolve(), offset + args.start_byte))
            if resolved == content[args.start_byte:args.end_byte]:
                expected = len(args.named_children)
            if expected is None:
                continue
            if str(path).endswith('/save/block_entity_encode.rs') and name in ('item_to_nbt', 'encode_items'):
                continue
            if len(args.named_children) != expected:
                continue
            if name.endswith('.write') and args.named_children and content[args.named_children[0].start_byte:args.named_children[0].end_byte] in (b'true', b'false'):
                continue
            parent = origin.parent
            test = 'test' in path.stem or '/tests/' in str(path)
            game_method = False
            level_method = False
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
                    if receiver is not None:
                        receiver_type = source[receiver.start_byte:receiver.end_byte].removeprefix(b'super::').removeprefix(b'crate::game::')
                        game_method |= receiver_type == b'Game' or receiver_type.startswith(b'Game<')
                        level_method |= receiver_type == b'GameLevel' or receiver_type.startswith(b'GameLevel<')
                parent = parent.parent
            context = 'items'
            if test or '/examples/' in str(path):
                context = '&Default::default()'
                if str(path).endswith('/src/item_data.rs'):
                    context = 'prototype_items()'
            if game_method:
                context = 'self.res.items()'
            if level_method:
                context = 'self.items'
            if test and name.startswith('game.'):
                context = 'game.res.items()'
            if menu_drop:
                context = 'env.items'
            if arc_catalog:
                context = 'self.res.shared_items()' if game_method else 'items.clone()'
                if test or '/examples/' in str(path):
                    context = 'Default::default()'
            insertion = args.named_children[-1].end_byte if args.named_children else args.start_byte + 1
            separator = ', ' if args.named_children else ''
            changes.append((offset + insertion, f'{separator}{context}'.encode()))
        for offset, addition in sorted(changes, reverse=True):
            source = source[:offset] + addition + source[offset:]
        if source != path.read_bytes():
            path.write_bytes(source)

# Real parity fixtures already load the catalog; only synthetic tests use empties.
probes = Path('crates/sunrise-client/examples/probe')
for name in ('brewing_fixture', 'ghost_recipe_fixture', 'stack_mining_probe', 'recipe_fixture',
             'place_recipe_fixture', 'menu_fixture', 'trading_fixture', 'creative_search_fixture'):
    path = probes / f'{name}.rs'
    text = path.read_text()
    if name in ('menu_fixture', 'place_recipe_fixture'):
        text = text.replace('fn state(menu: &Menu) -> String {', 'fn state(menu: &Menu, catalog: &Items) -> String {')
        text = text.replace('state(&menu)', 'state(&menu, &items)')
        text = text.replace('&Default::default()', 'catalog')
    elif name == 'trading_fixture':
        text = text.replace('fn stack(s: Option<&ItemStack>) -> String {', 'fn stack(s: Option<&ItemStack>, items: &Items) -> String {')
        text = text.replace('item_to_nbt(s, &Default::default())', 'item_to_nbt(s, items)')
        for expression in ('menu.slot(i)', 'menu.carried.as_ref()', 'Some(st)'):
            text = text.replace(f'stack({expression})', f'stack({expression}, env.items)')
            text = text.replace(f'stack({expression}, &items)', f'stack({expression}, env.items)')
        text = text.replace('&Default::default()', '&items')
    else:
        text = text.replace('&Default::default()', '&items')
        if name == 'creative_search_fixture':
            text = text.replace('use std::{fs, path::PathBuf, sync::Arc};', 'use std::{fs, path::PathBuf};')
            text = text.replace('enchant::Enchantments,', 'items::Items,')
            text = text.replace('''    creative.set_enchantments(Arc::new(Enchantments::load(
        &resources,
        &sunrise_world::item_defaults::read(&resources)?,
    )?));''', '''    let defaults = sunrise_world::item_defaults::read(&resources)?;
    let items = Items::catalog(&resources, &sunrise_world::loot::PackOverlay::NONE, &defaults)?;
    creative.set_enchantments(items.enchantments.clone());''')
    path.write_text(text)
