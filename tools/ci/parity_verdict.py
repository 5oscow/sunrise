#!/usr/bin/env python3
"""Reduce this attempt's private parity log to harness names and outcomes for the daily summary.

One share: `parity_verdict.py FOLDER REPORT --bucket I/N`. Every share:
`parity_verdict.py --merge REPORT VERDICT... --expect N [--plan SELECTION]`, which is complete only when the
shares reported exactly once each, agree on the run, and cover the planned selection.
"""
import argparse
import json
import re
import shutil
import sys
from pathlib import Path

RECORD = re.compile(r"^(PASS|FAIL) ([a-z0-9_]+(?:#\d+)?) \d+s(?: |$)")
FINISHED = re.compile(r"^(\d+) passed, (\d+) failed in [\d.]+ min\.$")


def verdict(log, expected, status):
    """Latest outcome per selected shard. A footer alone does not prove the selected set ran."""
    labels, counts = {}, None
    for line in log.splitlines():
        if (found := RECORD.match(line)) and found[2] in expected:
            labels[found[2]] = found[1] == "PASS"
        if finished := FINISHED.fullmatch(line):
            counts = (int(finished[1]), int(finished[2]))
    passes = sum(labels.values())
    failures = len(labels) - passes
    complete = (bool(expected) and labels.keys() == expected and counts == (passes, failures)
                and status == str(int(failures != 0)))
    harnesses = {}
    for label, passed in labels.items():
        name = label.split("#", 1)[0]
        harnesses[name] = harnesses.get(name, True) and passed
    # An incomplete run proves no harness passed: its passes stay out of the uploaded verdict.
    return ({"passed": sorted(name for name, passed in harnesses.items() if passed) if complete else [],
             "failed": sorted(name for name, passed in harnesses.items() if not passed),
             "complete": complete}, labels)


def merge(reports, expected, plan=None):
    """One verdict from the shares' verdicts.

    A run is complete only when each expected share reported exactly once, every report finished, they agree
    on the run, and their selections are disjoint and cover the planned selection. Anything else is an
    unfinished run: it keeps the failures it saw, and proves no pass."""
    by_share, problems = {}, []
    for report in reports:
        share = report.get("bucket")
        if not isinstance(share, int):
            if expected != 1:
                problems.append("a report names no share")
                continue
            share = 0
        if share in by_share:
            problems.append(f"share {share} reported twice")
            continue
        by_share[share] = report
    shares = sorted(by_share)
    if shares != list(range(expected)):
        problems.append(f"shares {shares or 'none'} reported, not 0..{expected - 1}")
    runs = {json.dumps(by_share[share].get("scope"), sort_keys=True) for share in shares}
    if len(runs) != 1 or "null" in runs:
        problems.append("the shares do not agree on the run")
    selected = []
    for share in shares:
        report = by_share[share]
        selected.extend(report.get("selected") or ())
        if not report.get("complete"):
            problems.append(f"share {share} did not finish")
    if plan is not None:
        if len(selected) != len(set(selected)):
            problems.append("two shares ran the same harness")
        if set(selected) != set(plan):
            problems.append(f"{len(set(plan) - set(selected))} planned harnesses ran in no share, "
                            f"{len(set(selected) - set(plan))} ran outside the plan")
    complete = not problems
    harnesses = {}
    for report in reports:
        for name in report.get("passed", ()):
            harnesses[name] = harnesses.get(name, True)
        for name in report.get("failed", ()):
            harnesses[name] = False
    return {
        "passed": sorted(name for name, passed in harnesses.items() if passed) if complete else [],
        "failed": sorted(name for name, passed in harnesses.items() if not passed),
        "complete": complete,
        "problems": problems,
        "shares": {"reported": len(reports), "expected": expected},
        "scope": json.loads(runs.pop()) if len(runs) == 1 and "null" not in runs else None,
    }


def main(*arguments):
    if arguments and arguments[0] == "--merge":
        return merge_main(arguments[1:])
    parser = argparse.ArgumentParser(description="Reduce one share's private parity log to a verdict.")
    parser.add_argument("folder")
    parser.add_argument("report")
    parser.add_argument("--bucket", metavar="I/N", help="which share of the run this log is")
    args = parser.parse_args(arguments)
    share = None
    if args.bucket:
        try:
            index, count = (int(part) for part in args.bucket.split("/", 1))
        except ValueError:
            parser.error(f"--bucket is I/N, not {args.bucket}")
        if count < 1 or not 0 <= index < count:
            parser.error(f"--bucket is I/N with 0 <= I < N, not {args.bucket}")
        share = {"index": index, "of": count}
    folder = Path(args.folder)
    run_id, run_attempt, commit = (folder / "scope").read_text().splitlines()
    log, selection, exit_status = (folder / name for name in ("parity.log", "selection.json", "exit"))
    expected = set(json.loads(selection.read_text())) if selection.is_file() else set()
    result, labels = verdict(log.read_text(errors="replace") if log.is_file() else "", expected,
                             exit_status.read_text().strip() if exit_status.is_file() else "")
    result["selected"] = sorted(expected)
    result["bucket"] = None if share is None else share["index"]
    result["of"] = 1 if share is None else share["of"]
    result["scope"] = {"run_id": run_id, "run_attempt": run_attempt, "commit": commit}
    Path(args.report).write_text(json.dumps(result, indent=2) + "\n")
    print_verdict(result)

    # Preserve failed-shard and first-retry logs across reuse of the slot's scratch directory.
    logs = Path(".local/parity-runner/logs")
    for path in [*logs.glob("*.first-run.log"), *(logs / f"{label}.log" for label, passed in labels.items() if not passed)]:
        if path.is_file():
            shutil.copyfile(path, folder / path.name)


def merge_main(arguments):
    parser = argparse.ArgumentParser(description="Merge the daily run's share verdicts into one parity.json.")
    parser.add_argument("report", help="the merged verdict to write")
    parser.add_argument("sources", nargs="*", help="each share's verdict")
    parser.add_argument("--expect", type=int, required=True, help="how many shares the run has")
    parser.add_argument("--plan", type=Path, help="the planned selection every share was carved from")
    args = parser.parse_args(arguments)
    reports = []
    for path in args.sources:
        try:
            reports.append(json.loads(Path(path).read_text()))
        except (OSError, ValueError):
            continue  # an unreadable report is a share that did not report; the count check names it
    plan = None
    if args.plan:
        try:
            plan = json.loads(args.plan.read_text())
        except (OSError, ValueError):
            plan = []  # no plan to cover: the coverage check says so, and the run is unfinished
    result = merge(reports, args.expect, plan)
    Path(args.report).write_text(json.dumps(result, indent=2) + "\n")
    print_verdict(result)
    for problem in result["problems"]:
        print(f"not complete: {problem}", file=sys.stderr)
    return 0


def print_verdict(result):
    for name in result["passed"] if result["complete"] else []:
        print(f"PASS {name}")
    for name in result["failed"]:
        print(f"FAIL {name}")


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]) or 0)
