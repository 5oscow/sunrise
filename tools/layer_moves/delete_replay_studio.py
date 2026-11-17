#!/usr/bin/env python3
"""Remove the built-in Replay Studio modules, exports, and translation keys."""

import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
CLIENT = ROOT / 'crates/sunrise-client'
RENDER = ROOT / 'crates/sunrise-client-render'
KEY = re.compile(r'^\s*"sunrise\.(?:studio\.|pages\.(?:open\.)?studio(?:\.tooltip)?")')


def main():
    for name in ('camera', 'mcpr', 'mod', 'path_render', 'recorder', 'replay', 'spline', 'timeline_ui', 'topbar'):
        (CLIENT / f'src/studio/{name}.rs').unlink()
    for name in ('packet_source', 'game/interface/studio', 'game/runtime/replay'):
        (CLIENT / f'src/{name}.rs').unlink()
    (RENDER / 'src/studio_ui.rs').unlink()

    for path, exports in (
        (CLIENT / 'src/client_lib.rs', ('pub mod packet_source;\n', 'pub mod studio;\n')),
        (RENDER / 'src/lib.rs', ('pub mod studio_ui;\n',)),
    ):
        source = path.read_text(encoding='utf-8')
        for export in exports:
            assert source.count(export) == 1, export
            source = source.replace(export, '')
        path.write_text(source, encoding='utf-8')

    for folder in (CLIENT / 'assets/sunrise/lang', ROOT / 'tools/lang_review/game'):
        for path in sorted(folder.glob('*.json')):
            original = path.read_text(encoding='utf-8')
            cleaned = ''.join(line for line in original.splitlines(keepends=True) if not KEY.match(line))
            if original != cleaned:
                cleaned = re.sub(r',(?=\s*}\s*\Z)', '', cleaned)
                json.loads(cleaned)
                path.write_text(cleaned, encoding='utf-8')

    manifest = ROOT / 'tools/ci/ignored-tests/daily-sunrise-client-issue-4451.txt'
    source = manifest.read_text(encoding='utf-8')
    test = 'sunrise-client game::chat::tests::studio_is_in_the_client_chat_tree_without_a_mod_registering_it lib daily resources\n'
    assert source.count(test) == 1, test
    manifest.write_text(source.replace(test, ''), encoding='utf-8')


if __name__ == '__main__':
    main()
