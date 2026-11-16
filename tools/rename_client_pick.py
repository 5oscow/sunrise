"""Rename the client pick seam now shared by tick and frame call sites."""
from pathlib import Path
import subprocess

paths = subprocess.check_output(['git', 'diff', '--name-only', '--diff-filter=ACMR', 'origin/main'], text=True).splitlines()
replacements = [
    ('ClientFramePick', 'ClientPick'),
    ('client_frame_pick', 'client_pick'),
    ('FramePick', 'ClientPick'),
    ('refresh_frame_pick', 'refresh_client_pick'),
    ('stored_frame_pick', 'stored_client_pick'),
    ('frame_pick_distance', 'pick_distance'),
    ('uses_frame_pick', 'uses_stored_pick'),
    ('frame_pick_live', 'pick_target_live'),
    ('accept_frame_pick', 'accept_client_pick'),
    ('.frame_pick', '.client_pick'),
    ('frame_pick:', 'client_pick:'),
]
for name in paths:
    path = Path(name)
    if path.suffix not in {'.rs', '.md', '.toml', '.txt'}:
        continue
    source = path.read_text()
    for old, new in replacements:
        source = source.replace(old, new)
    path.write_text(source)
for old, new in [
    ('crates/sunrise-client/src/game/native_hooks/client_frame_pick.rs', 'crates/sunrise-client/src/game/native_hooks/client_pick.rs'),
    ('crates/sunrise-client/src/game/native_hooks/client_frame_pick', 'crates/sunrise-client/src/game/native_hooks/client_pick'),
    ('crates/sunrise-mod-vocab/src/hooks/client_frame_pick.rs', 'crates/sunrise-mod-vocab/src/hooks/client_pick.rs'),
    ('crates/sunrise-mod-vocab/changes/client-frame-pick.toml', 'crates/sunrise-mod-vocab/changes/client-pick.toml'),
    ('tools/ci/ignored-tests/client-frame-pick.txt', 'tools/ci/ignored-tests/client-pick.txt'),
]:
    Path(old).rename(new)
