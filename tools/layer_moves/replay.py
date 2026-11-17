#!/usr/bin/env python3
"""Move the replay archive, recorder and packet source out of the client.

Run from the repository root, on the parent of the layer-move commit.
"""

from pathlib import Path

source = Path("crates/sunrise-client/src/studio")
destination = Path("crates/sunrise-replay/src")
destination.mkdir(parents=True, exist_ok=True)
for name in ("mcpr.rs", "recorder.rs", "replay.rs"):
    target = destination / name
    if target.exists():
        raise FileExistsError(target)
    (source / name).rename(target)
