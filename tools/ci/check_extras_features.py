#!/usr/bin/env python3
"""Check Linux release-game and SDK feature resolution without compiling either variant."""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BUILD_ROOTS = ("sunrise-game", "sunrise-mod-host")
# The release workflows will use this same feature set when they build both variants.
EXTRAS_OFF_FEATURES = "audio,controllers"
EXTRAS_OWNERS = {"sunrise-client", "sunrise-game", "sunrise-mod-host"}


def features_for(root, *, extras_off):
    command = ["cargo", "tree", "--locked", "--color", "never", "-p", root,
               "-e", "normal,build", "--prefix", "none", "--format", "{p}|{f}"]
    if extras_off:
        command += ["--no-default-features", "--features", EXTRAS_OFF_FEATURES]
    output = subprocess.check_output(command, cwd=ROOT, text=True)
    packages = {}
    for line in output.splitlines():
        package, features = line.removesuffix(" (*)").split("|", 1)
        if package.split()[0] not in EXTRAS_OWNERS:
            # Build dependencies can have a separate feature set from normal dependencies.
            packages.setdefault(package, set()).add(tuple(sorted(filter(None, features.split(",")))))
    return packages


def describe(feature_sets):
    return " / ".join(f"[{', '.join(features)}]" for features in sorted(feature_sets))


def main():
    failed = False
    for root in BUILD_ROOTS:
        default = features_for(root, extras_off=False)
        extras_off = features_for(root, extras_off=True)
        for package in sorted(default.keys() & extras_off.keys()):
            # The default builds first. A default-only host/target instance needs no rebuild.
            if not extras_off[package] <= default[package]:
                failed = True
                print(f"{root}: {package}: default {describe(default[package])}; "
                      f"Extras-off {describe(extras_off[package])}", file=sys.stderr)
    if not failed:
        print("Shared dependency features match for default and Extras-off (sunrise-game and sunrise-mod-host).")
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
