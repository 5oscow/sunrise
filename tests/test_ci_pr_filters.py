"""checks starts on every pull request and skips a job only when the complete diff leaves its inputs untouched."""
from pathlib import Path
import re
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/ci/vendor/pyyaml-6.0.3.zip"))
import yaml  # noqa: E402

# Each job a fork's pull request can skip, and the selections that keep it.
GATED = {"verify-tree": ("tooling_required", "rust_required"), "verify-lint": ("rust_required",),
         "verify-test": ("rust_required",), "launcher": ("launcher_required",)}


def workflow(name="ci"):
    return yaml.load((ROOT / f".github/workflows/{name}.yml").read_text(), Loader=yaml.BaseLoader)


def evaluate(expression, context):
    """Evaluate the selection predicates from the workflow for a synthetic Actions context."""
    expression = " ".join(expression.split()).removeprefix("${{").removesuffix("}}").strip()
    expression = re.sub(r"[a-zA-Z_][\w-]*(?:\.[\w*-]+)+", lambda m: repr(context.get(m[0], "")), expression)
    expression = expression.replace("&&", " and ").replace("||", " or ")
    expression = re.sub(r"!(?!=)", " not ", expression).strip()
    return eval(expression, {"__builtins__": {}}, {
        "always": lambda: True, "cancelled": lambda: False,
        "startsWith": str.startswith, "contains": lambda values, item: item in values,
    })


class PullRequestSelectionTests(unittest.TestCase):
    def test_the_trigger_has_no_path_filter(self):
        # GitHub matches a path filter against the first 300 changed files only: a Rust change later in a long list
        # would not start the workflow.
        on = workflow()["on"]
        self.assertEqual(set(on), {"pull_request", "schedule", "workflow_dispatch"})
        self.assertFalse({"paths", "paths-ignore"} & set(on["pull_request"]))

    def test_each_job_skips_only_on_its_own_selection(self):
        jobs = workflow()["jobs"]
        changes = jobs["changes"]
        self.assertIn("github.event.pull_request.head.repo.full_name != github.repository", changes["if"])
        select = next(step for step in changes["steps"] if step.get("id") == "selection")
        self.assertEqual(select["run"],
                         'python3 tools/ci/rust_changes.py --all "$BASE_SHA" "$HEAD_SHA" >> "$GITHUB_OUTPUT"')
        for job, outputs in GATED.items():
            with self.subTest(job=job):
                self.assertEqual(jobs[job]["needs"], "changes")
                condition = jobs[job]["if"]
                # A skipped or failed selection leaves the outputs empty: `!= 'false'` then runs the job, and
                # `!cancelled()` keeps a skipped `changes` (the schedule) from skipping it.
                self.assertTrue(condition.startswith("!cancelled() && (github.event_name == 'schedule' ||"))
                for output in outputs:
                    self.assertIn(f"needs.changes.outputs.{output} != 'false'", condition)
                self.assertNotIn("== 'true'", condition)

    def test_failed_or_missing_selection_does_not_skip_or_rerun_fork_checks(self):
        jobs = workflow()["jobs"]
        event = {"github.event_name": "pull_request", "github.repository": "org/game",
                 "github.event.pull_request.head.repo.full_name": "fork/game"}
        output_keys = {name for outputs in GATED.values() for name in outputs}
        for result, outcome, value, expected in (
            ("success", "success", "false", False), ("success", "success", "true", True),
            ("success", "success", "", True), ("success", "failure", "false", True),
            ("failure", "success", "false", True),
        ):
            context = dict(event, **{"needs.changes.result": result})
            producer = {"steps.selection.outcome": outcome,
                        **{f"steps.selection.outputs.{key}": value for key in output_keys}}
            context.update({f"needs.changes.outputs.{key}": evaluate(jobs["changes"]["outputs"][key], producer)
                            for key in output_keys})
            for name in GATED:
                with self.subTest(name=name, result=result, outcome=outcome, value=value):
                    self.assertEqual(evaluate(jobs[name]["if"], context), expected)
                    self.assertFalse(any("rust_changes.py" in step.get("run", "") for step in jobs[name]["steps"]))
        for name, command in (("verify-lint", "fmt --all"), ("verify-test", "test --locked"),
                              ("launcher", "test --locked")):
            with self.subTest(name=name):
                self.assertTrue(any(command in step.get("run", "") and "if" not in step for step in jobs[name]["steps"]))


if __name__ == "__main__":
    unittest.main()
