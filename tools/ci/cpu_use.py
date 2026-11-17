"""Measure the runner's own CPU use over a stretch of a job, from its cgroup v2 `cpu.stat`.

`mark NAME` saves the counters; `report NAME LABEL` prints, for the time since that mark, the CPU time the runner's
processes used against what its quota (`cpu.max`) allowed, and how much of that time the quota throttled them.
The parity job runs it around its probe build and its harnesses, to size `-j` from measured use (numbers only).
"""
import argparse
import os
import time
from pathlib import Path

CGROUP = Path("/sys/fs/cgroup")


def counters() -> dict[str, int]:
    """cpu.stat's counters, and the wall clock in microseconds."""
    values = dict(line.split() for line in (CGROUP / "cpu.stat").read_text().splitlines())
    return {name: int(value) for name, value in values.items()} | {"wall_usec": time.monotonic_ns() // 1000}


def cpus() -> float:
    """The runner's CPU quota; the CPUs it may run on where it has none."""
    quota, period = (CGROUP / "cpu.max").read_text().split()
    return len(os.sched_getaffinity(0)) if quota == "max" else int(quota) / int(period)


def report(before: dict[str, int], after: dict[str, int], quota: float) -> str:
    delta = {name: after[name] - before.get(name, 0) for name in after}
    wall = delta["wall_usec"] / 1e6
    used = delta["usage_usec"] / 1e6
    line = (f"{used:.0f} CPU-s in {wall:.0f} s on {quota:g} CPUs: {100 * used / (quota * wall):.0f}% of the quota"
            if wall > 0 else "no time passed")
    if "nr_periods" in delta and delta["nr_periods"]:
        line += (f", throttled {delta['throttled_usec'] / 1e6:.0f} s in {delta['nr_throttled']} of "
                 f"{delta['nr_periods']} periods")
    return line


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["mark", "report"])
    parser.add_argument("name")
    parser.add_argument("label", nargs="?", default="")
    args = parser.parse_args()
    saved = Path(os.environ["RUNNER_TEMP"]) / f"cpu-{args.name}.txt"
    if args.action == "mark":
        saved.write_text("".join(f"{name} {value}\n" for name, value in counters().items()))
        return
    before = {name: int(value) for name, value in (line.split() for line in saved.read_text().splitlines())}
    line = f"{args.label or args.name}: {report(before, counters(), cpus())}"
    print(line)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write(f"- {line}\n")


if __name__ == "__main__":
    main()
