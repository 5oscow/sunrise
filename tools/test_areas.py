#!/usr/bin/env python3
"""Code areas and the tests that cover them, from tools/test_areas.toml.

  python3 tools/test_areas.py list                           # every area and what it runs
  python3 tools/test_areas.py list --changed origin/main     # the areas a branch touches
  python3 tools/test_areas.py list --changed-files paths.txt # the same for listed paths
  python3 tools/test_areas.py run explosions                 # its Rust tests, then its harnesses
  python3 tools/test_areas.py run explosions --daily --lan   # also its daily tests and its LAN checks
  python3 tools/test_areas.py run --changed origin/main      # the touched areas; harnesses as parity.py --changed picks them
  python3 tools/test_areas.py coverage                       # tracked crate files no area claims

Picking from a diff stays with tools/parity.py --changed (harnesses) and tools/ci/test_scope.py (crates). An area
names a set of tests, adds the daily and LAN tests nothing picks from a diff, and is how to ask for "the explosion
tests". `--daily` needs the prepared data (SUNRISE_TEST_RESOURCES), as the daily run does; `--lan` builds and starts
two games (tools/lan_e2e.py), and the network area holds every LAN check (~40 minutes).
"""

import argparse
import fnmatch
import re
import subprocess
import sys
import tomllib
from collections import defaultdict
from pathlib import Path
from typing import NamedTuple

ROOT = Path(__file__).resolve().parents[1]
# The repository root, appended so a script run finds `tools` without shadowing anything already importable.
sys.path.append(str(ROOT))

MAP = ROOT / "tools" / "test_areas.toml"
LAN = ROOT / "tools" / "lan_e2e.py"
# lan_e2e.py's check names: `CHECKS = [...]` and the `CHECKS += [...]` beside each later check.
CHECK_LISTS = re.compile(r"^CHECKS\s*\+?=\s*\[(.*?)\]", re.M | re.S)
QUOTED = re.compile(r"'([^']+)'")
HARNESS = re.compile(r"^tools/test_(\w+)_parity\.py$")
TARGET = re.compile(r"^(?:lib|(?:bin|test):[\w-]+)$")


class Cargo(NamedTuple):
    package: str
    target: str  # `lib`, `bin:NAME` or `test:NAME`, as the daily rows name targets
    filter: str  # a libtest filter: a substring of the test's path; empty for all of the target's tests


class Area(NamedTuple):
    name: str
    about: str
    paths: tuple[str, ...]
    cargo: tuple[Cargo, ...]
    harnesses: tuple[str, ...]
    lan: tuple[str, ...]


class Map(NamedTuple):
    areas: tuple[Area, ...]
    escalate: tuple[str, ...]
    unassigned: tuple[str, ...]  # harness names no area claims yet


class Selection(NamedTuple):
    areas: tuple[Area, ...]
    unmapped: tuple[str, ...]  # changed paths no area claims
    escalated: tuple[str, ...]  # changed paths that can reach every area


def load(path=MAP):
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    areas = tuple(Area(
        area["name"], area.get("about", ""), tuple(area.get("paths", ())),
        tuple(Cargo(entry["package"], entry["target"], entry.get("filter", "")) for entry in area.get("cargo", ())),
        tuple(area.get("harnesses", ())), tuple(area.get("lan", ())),
    ) for area in data["area"])
    return Map(areas, tuple(data.get("escalate", ())), tuple(data.get("unassigned", {}).get("harnesses", ())))


def matching(patterns, names):
    """The names any pattern matches, in their own order (fnmatch: `*` also crosses `/`)."""
    return [name for name in names if any(fnmatch.fnmatchcase(name, pattern) for pattern in patterns)]


def harness_names():
    """Every harness, as tools/parity.py names them."""
    return sorted(path.name[len("test_"):-len("_parity.py")] for path in (ROOT / "tools").glob("test_*_parity.py"))


def lan_checks():
    """Every tools/lan_e2e.py check, read from its source: importing it would load the whole driver."""
    text = LAN.read_text(encoding="utf-8")
    return [name for block in CHECK_LISTS.findall(text) for name in QUOTED.findall(block)]


def tracked():
    """Tracked files, and new ones not yet added (a patch applied but not committed)."""
    return subprocess.run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT,
                          capture_output=True, text=True, check=True).stdout.split("\0")[:-1]


def daily_rows():
    """The daily rows (tools/ci/ignored-tests/) the daily run runs: what `run --daily` chooses from."""
    from tools.ci import ignored_tests
    return ignored_tests.daily_rows()


def covers(entry, package, target, test):
    return entry.package == package and entry.target == target and entry.filter in test


def target_flags(target):
    kind, _, name = target.partition(":")
    return ["--lib"] if kind == "lib" else [f"--{kind}", name]


def select(table, paths):
    """The areas changed paths touch, in the map's order, and the paths no area claims."""
    chosen, unmapped, escalated = set(), [], []
    for path in paths:
        if matching(table.escalate, [path]):
            escalated.append(path)
            continue
        hits = {area.name for area in table.areas if matching(area.paths, [path])}
        # A harness script belongs to the areas that list the harness.
        if harness := HARNESS.match(path):
            hits |= {area.name for area in table.areas if matching(area.harnesses, [harness[1]])}
        # Documentation nobody builds from selects nothing, as in tools/ci/test_scope.py.
        inert = (path.startswith("docs/") or path.endswith(".md")) and not path.startswith(".github/")
        if not hits and not inert:
            unmapped.append(path)
        chosen |= hits
    return Selection(tuple(area for area in table.areas if area.name in chosen), tuple(unmapped), tuple(escalated))


def plan(area, harnesses, checks, rows, daily=False, lan=False):
    """The commands that run one area's tests, cheapest first, and what each tier holds."""
    commands = []
    for entry in area.cargo:
        commands.append(["cargo", "test", "-p", entry.package, *target_flags(entry.target),
                         *(["--", entry.filter] if entry.filter else [])])
    chosen = [row for row in rows if any(covers(entry, row.package, row.reference, row.test) for entry in area.cargo)]
    if daily:
        from tools.ci import ignored_tests
        groups = defaultdict(list)
        for row in chosen:
            groups[row.package, row.reference].append(row.test)
        for (package, target), tests in sorted(groups.items()):
            # As the daily run builds them: the workspace's features where the package's own shard doesn't run them.
            features = [] if ignored_tests.shard_runs(package, target) else \
                ignored_tests.workspace_features().get(package, [])
            commands.append(["cargo", "test", "-p", package, *(["--features", ",".join(features)] if features else []),
                             *target_flags(target), "--", "--ignored", "--exact", *sorted(tests)])
    named = matching(area.harnesses, harnesses)
    if named:
        commands.append([sys.executable, "tools/parity.py", *named])
    picked = matching(area.lan, checks)
    if lan and picked:
        commands.append([sys.executable, "tools/lan_e2e.py", *(flag for check in picked for flag in ("--check", check))])
    return commands, {"daily": chosen, "harnesses": named, "lan": picked}


def describe(area, tiers):
    reasons = defaultdict(int)
    for row in tiers["daily"]:
        reasons[row.reason] += 1
    lines = [f"{area.name}: {area.about}"]
    lines += [f"  rust       cargo test -p {entry.package} {' '.join(target_flags(entry.target))}"
              + (f" -- {entry.filter}" if entry.filter else "") for entry in area.cargo]
    lines.append(f"  daily      {len(tiers['daily'])} tests"
                 + (f" ({', '.join(f'{reason} {count}' for reason, count in sorted(reasons.items()))})" if reasons else ""))
    lines.append(f"  harnesses  {len(tiers['harnesses'])}: {' '.join(tiers['harnesses'])}")
    lines.append(f"  lan        {len(tiers['lan'])}: {' '.join(tiers['lan'])}")
    return "\n".join(lines)


def changed(args):
    """The changed paths --changed or --changed-files names, or None for neither."""
    if args.changed:
        from tools import parity
        paths = parity.changed_files(args.changed)
        if paths is None:
            sys.exit(f"no merge base with {args.changed}")
        return sorted(paths)
    if args.changed_files:
        return [line.strip() for line in Path(args.changed_files).read_text(encoding="utf-8").splitlines() if line.strip()]
    return None


def report(selection):
    names = " ".join(area.name for area in selection.areas) or "none"
    print(f"areas: {names}")
    if selection.escalated:
        print(f"these reach every area, so run the full set (tools/pr_ready.py): {' '.join(selection.escalated)}")
    if selection.unmapped:
        print(f"in no area ({len(selection.unmapped)}), so their crates' tests are not run here "
              f"(tools/ci/test_scope.py names them): {' '.join(selection.unmapped)}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("list", "run"):
        command = commands.add_parser(name)
        command.add_argument("areas", nargs="*", help="area names (default: all for list, none for run)")
        source = command.add_mutually_exclusive_group()
        source.add_argument("--changed", metavar="REF", help="the areas the changes since the merge base with REF touch")
        source.add_argument("--changed-files", metavar="FILE", help="the areas these paths (one per line) touch")
        command.add_argument("--daily", action="store_true", help="also the area's daily tests (needs prepared data)")
        command.add_argument("--lan", action="store_true", help="also the area's LAN checks (starts two games)")
    coverage = commands.add_parser("coverage")
    coverage.add_argument("--files", action="store_true", help="list each tracked file no area claims")
    args = parser.parse_args(argv)
    table = load()

    if args.command == "coverage":
        files = [path for path in tracked() if path.startswith("crates/")]
        loose = [path for path in files if not any(matching(area.paths, [path]) for area in table.areas)]
        by_crate = defaultdict(int)
        for path in loose:
            by_crate["/".join(path.split("/")[:2])] += 1
        print(f"{len(files) - len(loose)} of {len(files)} files under crates/ are in an area")
        for crate, count in sorted(by_crate.items(), key=lambda item: -item[1]):
            print(f"  {count:5}  {crate}")
        if args.files:
            print("\n".join(loose))
        return 0

    known = {area.name: area for area in table.areas}
    if unknown := [name for name in args.areas if name not in known]:
        parser.error(f"no area {', '.join(unknown)}; the areas: {', '.join(known)}")
    paths = changed(args)
    if paths is not None:
        if args.areas:
            parser.error("name areas, or pass --changed/--changed-files, not both")
        selection = select(table, paths)
        report(selection)
        areas = selection.areas
    elif args.areas:
        areas = tuple(known[name] for name in args.areas)
    elif args.command == "list":
        areas = table.areas
    else:
        parser.error("run needs area names, --changed or --changed-files")

    harnesses, checks, rows = harness_names(), lan_checks(), daily_rows()
    plans = [(area, *plan(area, harnesses, checks, rows, args.daily, args.lan)) for area in areas]
    if args.command == "list":
        for area, _, tiers in plans:
            print(describe(area, tiers))
        return 0

    steps = []
    for _, area_commands, _ in plans:
        steps += [command for command in area_commands if command[1:2] != ["tools/parity.py"]]
    if paths is not None:
        # From a diff, harnesses come from the runner's traced dependencies, not the areas' lists.
        source = ["--changed", args.changed] if args.changed else ["--changed-files", str(Path(args.changed_files).resolve())]
        steps.append([sys.executable, "tools/parity.py", *source])
    else:
        named = sorted({name for _, _, tiers in plans for name in tiers["harnesses"]})
        if named:
            steps.append([sys.executable, "tools/parity.py", *named])
    # One LAN run for every chosen check: each run starts two games.
    lan = [command for command in steps if command[1:2] == ["tools/lan_e2e.py"]]
    if lan:
        wanted = {flag for command in lan for flag in command[2:] if flag != "--check"}
        checks_wanted = [check for check in checks if check in wanted]  # the driver's own order
        steps = [command for command in steps if command not in lan]
        steps.append([sys.executable, "tools/lan_e2e.py", *(flag for check in checks_wanted for flag in ("--check", check))])

    failed = []
    for command in dict.fromkeys(tuple(command) for command in steps):
        print("$ " + " ".join(command), flush=True)
        if subprocess.run(command, cwd=ROOT).returncode != 0:
            failed.append(" ".join(command))
    if failed:
        print("failed:\n  " + "\n  ".join(failed))
        return 1
    print("passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
