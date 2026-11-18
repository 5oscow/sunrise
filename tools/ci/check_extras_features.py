#!/usr/bin/env python3
"""Check that the default and vanilla (Extras-off) builds resolve every shared dependency with the same features,
so CI builds the second variant incrementally: a dependency whose features differ recompiles, and so does everything
above it. Resolves only (`cargo tree`), never compiles. Covers Linux, the platform it runs on; each build context
(the target, and the host that runs build scripts and proc-macros) is compared on its own, as Cargo compiles them
apart."""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BUILD_ROOTS = ("sunrise-game", "sunrise-mod-host")
# The vanilla build: Extras off, and controllers with it (an Extras feature). The release workflows will use this
# same feature set when they build both variants.
EXTRAS_OFF_FEATURES = "audio"
# `cargo tree --charset ascii`'s indent: four characters a level, then the package or a section header.
LINE = re.compile(r"^((?:[|` ] {3}|[|`]-- )*)(.*)$")
PATH = re.compile(r" \((/[^)]*)\)$")


def features_for(root, *, extras_off):
    """{(context, package): {features}}, context `target` or `host`, for every dependency outside the workspace:
    workspace crates own the Extras features, so theirs differ by design."""
    command = ["cargo", "tree", "--locked", "--color", "never", "--charset", "ascii", "-p", root,
               "-e", "normal,build", "--format", "{p}|{f}"]
    if extras_off:
        command += ["--no-default-features", "--features", EXTRAS_OFF_FEATURES]
    output = subprocess.check_output(command, cwd=ROOT, text=True)
    packages = {}
    # The current node at each depth: whether it is built for the host, and whether the children listed after it
    # are its build-dependencies (a `[build-dependencies]` header, indented as its children's continuation lines).
    host = []
    building = set()
    for line in output.splitlines():
        prefix, text = LINE.fullmatch(line).groups()
        depth = len(prefix) // 4
        if text == "[build-dependencies]":
            building.add(depth)
            continue
        building = {level for level in building if level < depth}
        package, features = text.removesuffix(" (*)").split("|", 1)
        # A build script's dependencies and a proc-macro, and all below them, run on the host.
        on_host = depth > 0 and (host[depth - 1] or depth - 1 in building) or package.endswith(" (proc-macro)")
        host[depth:] = [on_host]
        path = PATH.search(package.removesuffix(" (proc-macro)"))
        if path and Path(path[1]).is_relative_to(ROOT):
            continue
        context = "host" if on_host else "target"
        packages.setdefault((context, package), set()).add(tuple(sorted(filter(None, features.split(",")))))
    return packages


def describe(feature_sets):
    return " / ".join(f"[{', '.join(features)}]" for features in sorted(feature_sets))


def main():
    failed = False
    for root in BUILD_ROOTS:
        default = features_for(root, extras_off=False)
        extras_off = features_for(root, extras_off=True)
        for context, package in sorted(default.keys() & extras_off.keys()):
            if extras_off[context, package] != default[context, package]:
                failed = True
                print(f"{root}: {package} ({context}): default {describe(default[context, package])}; "
                      f"Extras-off {describe(extras_off[context, package])}", file=sys.stderr)
    if not failed:
        print("Shared dependency features match for default and Extras-off (sunrise-game and sunrise-mod-host).")
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
