#!/usr/bin/env python3
"""Keep Linux release executables runnable on Ubuntu 22.04 (docs/ci-runners.md)."""
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

# Ubuntu 22.04's libc. Newer builders must never silently raise this floor.
FLOOR = (2, 35)


def incompatible_versions(report):
    """Version requirements, not the builder's installed libc, determine compatibility."""
    incompatible = set()
    needs = False
    for line in report.splitlines():
        if line.startswith("Version "):
            needs = line.startswith("Version needs section")
        if not needs:
            continue
        for name in re.findall(r"\bName:\s+(GLIBC_[A-Za-z0-9_.]+)", line):
            version = re.fullmatch(r"GLIBC_(\d+(?:\.\d+)+)", name)
            # ABI tags (e.g. GLIBC_ABI_DT_RELR, added in 2.36) and private versions
            # are not an Ubuntu 22.04 public ABI. Unknown tags must not pass silently.
            if version is None or tuple(map(int, version[1].split("."))) > FLOOR:
                incompatible.add(name)
    return sorted(incompatible)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", nargs="*")
    parser.add_argument("--sdk", type=Path, help="check every ELF in this SDK artifact tree")
    parser.add_argument("--std-dir", type=Path, help="the pinned rustc's target-libdir (required with --sdk)")
    args = parser.parse_args()
    binaries = args.binaries
    if args.sdk is not None:
        if args.std_dir is None:
            parser.error("--sdk requires --std-dir")
        sdk = []
        for path in sorted(args.sdk.rglob("*")):
            if path.is_file():
                with path.open("rb") as file:
                    if file.read(4) == b"\x7fELF":
                        sdk.append(path)
        if not sdk:
            parser.error(f"no ELF files in SDK {args.sdk}")
        std = sorted(args.std_dir.glob("libstd-*.so"))
        if not std:
            parser.error(f"no pinned libstd-*.so in {args.std_dir}")
        binaries += [str(path) for path in [*sdk, *std]]
    elif args.std_dir is not None:
        parser.error("--std-dir requires --sdk")
    if not binaries:
        parser.error("give at least one binary or an SDK")
    readelf = shutil.which("readelf")
    if readelf is None:
        parser.error("readelf is required (install binutils)")
    failed = False
    for binary in binaries:
        result = subprocess.run([readelf, "--wide", "--version-info", "--", binary], text=True, capture_output=True,
                                env={**os.environ, "LC_ALL": "C"})
        if result.returncode:
            print(result.stderr, file=sys.stderr, end="")
            failed = True
            continue
        incompatible = incompatible_versions(result.stdout)
        if incompatible:
            print(f"{binary}: requires {', '.join(incompatible)}; Linux releases must support glibc 2.35 (Ubuntu 22.04)", file=sys.stderr)
            failed = True
        else:
            print(f"{binary}: glibc requirements fit Ubuntu 22.04")
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
