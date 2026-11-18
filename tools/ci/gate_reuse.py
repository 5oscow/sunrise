"""Whether a main push may reuse a gate that already passed on its exact tree: `reused=true|false` and `run=<id>`.

Every gate's lint job records the tree it checked out as an artifact named `gate-tree-<tree>`: a pull_request run's
tree is GitHub's test merge, a dispatch's the pull request's head, a push's the commit's. A rollup merged into main
usually leaves main with the very tree its pull_request run built, linted and tested, and main's push run then ran
that whole gate again: two large runners for about 25 minutes and a medium one for 6, at the moment the next
rollup's gate wants them. When this repository's dyna-checks run on the same tree passed its gate (lint and verify
with their Rust steps run), the push takes its verdict: lint, verify and verify-static skip the Rust and tooling work
on the small runner, as for a change selection finds no Rust or tooling in.

Anything else (no such run, a run from a fork or another workflow, an API error) reuses nothing and the push runs the
whole gate.
"""

import json
import os
import subprocess
import sys

WORKFLOW = ".github/workflows/dyna-ci.yml"
GATE = ("lint", "verify", "verify-static")
# The job, and the step in it that shows the job did the Rust work rather than the documentation-only path.
RUST_STEPS = {"lint": "Lint native targets", "verify": "Test Rust core and voxel kernel correctness"}


def api(path: str) -> dict:
    return json.loads(subprocess.run(["gh", "api", path], check=True, capture_output=True, text=True).stdout)


def gate_passed(jobs: list[dict]) -> bool:
    named = {job["name"]: job for job in jobs}
    return all(name in named and named[name]["conclusion"] == "success" for name in GATE) and all(
        any(step["name"] == want and step["conclusion"] == "success" for step in named[name]["steps"])
        for name, want in RUST_STEPS.items())


def reuse(repository: str, sha: str, read=api) -> tuple[int | None, str]:
    """The run whose verdict the push at sha may take, and why (or why not)."""
    tree = read(f"repos/{repository}/commits/{sha}")["commit"]["tree"]["sha"]
    artifacts = read(f"repos/{repository}/actions/artifacts?name=gate-tree-{tree}&per_page=30")["artifacts"]
    for artifact in sorted(artifacts, key=lambda a: a["created_at"], reverse=True):
        run_id = artifact["workflow_run"]["id"]
        run = read(f"repos/{repository}/actions/runs/{run_id}")
        # Only this repository's own branches run our workflow file as written; a fork's run proves nothing.
        if run["path"] != WORKFLOW or run["head_repository"]["full_name"] != repository:
            continue
        if gate_passed(read(f"repos/{repository}/actions/runs/{run_id}/jobs?per_page=100")["jobs"]):
            return run_id, f"tree {tree[:10]} of {sha[:10]} passed the gate in run {run_id} ({run['event']})"
    return None, f"no passed gate of this repository's dyna-checks on tree {tree[:10]} of {sha[:10]}"


def main() -> None:
    try:
        run, why = reuse(os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_SHA"])
    except subprocess.CalledProcessError as error:
        run, why = None, f"GitHub API error, running the whole gate: {error.stderr.strip()}"
    print(f"reused={'true' if run else 'false'}")
    print(f"run={run or ''}")
    print(f"gate reuse: {why}", file=sys.stderr)
    with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
        summary.write(f"Gate reuse: {why}.\n")


if __name__ == "__main__":
    main()
