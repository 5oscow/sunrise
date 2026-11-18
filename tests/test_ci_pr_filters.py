"""checks starts on every pull request and skips a job only when the complete diff leaves its inputs untouched."""
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
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
        publish = next(step for step in jobs["changes"]["steps"] if step.get("id") == "selected")
        answers = ("rust_required", "launcher_required", "tooling_required", "certain")

        def published(outcome, values):
            """The job's outputs: what the publishing step writes for the selection step's outcome and outputs."""
            with tempfile.TemporaryDirectory() as directory:
                output = Path(directory, "output")
                env = dict(os.environ, OUTCOME=outcome, GITHUB_OUTPUT=str(output), **{
                    name: values.get(key, "") for name, key in
                    zip(("RUST", "LAUNCHER", "TOOLING", "CERTAIN"), answers)})
                subprocess.run(["bash", "-e", "-c", publish["run"]], env=env, check=True, stdout=subprocess.DEVNULL)
                written = dict(line.split("=", 1) for line in output.read_text().splitlines())
            return {f"needs.changes.outputs.{key}": written[key] for key in answers}

        narrowed = dict.fromkeys(answers, "false")
        for result, outcome, values, expected in (
            ("success", "success", narrowed, False),
            ("success", "success", dict.fromkeys(answers, "true"), True),
            ("success", "success", dict.fromkeys(answers, ""), True),
            # A step that answered some of its questions: the rest are not "maybe false".
            ("success", "success", dict(narrowed, launcher_required=""), True),
            ("success", "success", dict(narrowed, certain="maybe"), True),
            ("success", "failure", narrowed, True), ("success", "skipped", narrowed, True),
            ("failure", "success", narrowed, True),
        ):
            context = dict(event, **{"needs.changes.result": result}, **published(outcome, values))
            for name in GATED:
                with self.subTest(name=name, result=result, outcome=outcome, values=values):
                    self.assertEqual(evaluate(jobs[name]["if"], context), expected)
                    self.assertFalse(any("rust_changes.py" in step.get("run", "") for step in jobs[name]["steps"]))
        self.assertEqual({key: f"${{{{ steps.selected.outputs.{key} }}}}" for key in answers},
                         jobs["changes"]["outputs"])
        for name, command in (("verify-lint", "fmt --all"), ("verify-test", "test --locked"),
                              ("launcher", "test --locked")):
            with self.subTest(name=name):
                self.assertTrue(any(command in step.get("run", "") and "if" not in step for step in jobs[name]["steps"]))

    def test_the_crate_scope_narrows_only_a_certain_selection(self):
        scope = next(step for step in workflow()["jobs"]["verify-test"]["steps"] if step.get("id") == "scope")
        self.assertIn("needs.changes.outputs.certain == 'true'", scope["if"])


if __name__ == "__main__":
    unittest.main()
