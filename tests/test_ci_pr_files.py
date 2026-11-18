"""The API's file list stands for git's diff only when it can't differ from it (tools/ci/pr_files.py)."""
import contextlib
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from tools.ci import pr_files
from tools.ci.rust_changes import changed_entries, listed_entries

BASE, HEAD = "b" * 40, "c" * 40
ENV = {"GITHUB_REPOSITORY": "o/r", "BASE_SHA": BASE, "HEAD_SHA": HEAD}


def api(files, base=BASE):
    """A stand-in for GitHub's comparison of the run's base and head: every file on its first page."""
    def get(path):
        assert path == f"compare/{BASE}...{HEAD}?per_page=1", path
        return {"base_commit": {"sha": base}, "files": files}
    return get


class PrFilesTests(unittest.TestCase):
    def run_main(self, get, changed, **env):
        with tempfile.TemporaryDirectory() as directory, \
                mock.patch.dict(os.environ, dict(ENV, CHANGED_FILES=str(changed), **env)), \
                mock.patch.object(pr_files, "get", side_effect=get), \
                contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()):
            listing = Path(directory, "files")
            pr_files.main(str(listing))
            return out.getvalue(), listed_entries(listing) if listing.exists() else None

    def test_the_api_list_matches_gits_diff_without_renames(self):
        with tempfile.TemporaryDirectory() as directory:
            def git(*args):
                return subprocess.check_output(["git", "-C", directory, "-c", "user.name=t", "-c", "user.email=t@t",
                                                "-c", "commit.gpgsign=false", *args], text=True).strip()
            git("init", "-q")
            for name in ["modified.md", "deleted.md", "old.rs", "kept.txt"]:
                Path(directory, name).write_text(f"{name}\n" * 20)
            git("add", ".")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD")
            Path(directory, "modified.md").write_text("changed\n")
            Path(directory, "deleted.md").unlink()
            Path(directory, "docs").mkdir()
            Path(directory, "old.rs").rename(Path(directory, "docs/new.rs"))
            Path(directory, "added.md").write_text("added\n")
            Path(directory, "copy.txt").write_text("kept.txt\n" * 20)
            git("add", "-A")
            git("commit", "-qm", "head")
            previous = Path.cwd()
            try:
                os.chdir(directory)
                expected = changed_entries(base, git("rev-parse", "HEAD"))
            finally:
                os.chdir(previous)
        # What GitHub lists for the same change: renames folded, a copy marked as one.
        files = [{"status": "modified", "filename": "modified.md"}, {"status": "removed", "filename": "deleted.md"},
                 {"status": "renamed", "filename": "docs/new.rs", "previous_filename": "old.rs"},
                 {"status": "added", "filename": "added.md"}, {"status": "copied", "filename": "copy.txt"}]
        out, listed = self.run_main(api(files), len(files))
        self.assertEqual(out, "listed=true\n")
        self.assertEqual(sorted(listed), sorted(expected))

    def test_a_same_count_push_and_revert_cannot_replace_the_event_comparison(self):
        # The old mutable PR endpoint could report the expected head both before and after a files request,
        # but serve a different same-count diff between the two reads.
        event = [{"status": "modified", "filename": "src/lib.rs"}]
        moved = [{"status": "modified", "filename": "docs/setup.md"}]

        def get(path):
            if path.startswith("pulls/"):
                return moved if "/files" in path else {"base": {"sha": BASE}, "head": {"sha": HEAD}}
            return api(event)(path)

        self.assertEqual(self.run_main(get, len(moved)), ("listed=true\n", [("M", "src/lib.rs")]))

    def test_a_comparison_below_the_limit_can_be_used(self):
        files = [{"status": "added", "filename": f"docs/{n}.md"} for n in range(pr_files.API_LIMIT - 1)]
        out, listed = self.run_main(api(files), len(files))
        self.assertEqual(out, "listed=true\n")
        self.assertEqual(len(listed), pr_files.API_LIMIT - 1)

    def test_empty_and_malformed_api_records_fall_back(self):
        for files in ([], [{"status": "modified", "filename": ""}],
                      [{"status": "modified", "filename": "docs/a.md\0M\0docs/b.md"}],
                      [{"status": "renamed", "filename": "docs/new.md", "previous_filename": None}],
                      [{"status": "modified", "filename": "docs/a.md"}] * 2):
            with self.subTest(files=files):
                self.assertEqual(self.run_main(api(files), len(files)), ("listed=false\n", None))

    def test_any_doubt_falls_back_to_the_history(self):
        one = [{"status": "modified", "filename": "docs/setup.md"}]
        for name, get, changed in [
            ("a count other than the event's", api(one), 2),
            ("the comparison's limit", api(one), pr_files.API_LIMIT),
            ("a comparison of another base", api(one, base="n" * 40), 1),
            ("a status git has no exact twin for", api([{"status": "changed", "filename": "docs/a.md"}]), 1),
            ("an API error", mock.Mock(side_effect=OSError("HTTP 502")), 1),
        ]:
            with self.subTest(name):
                self.assertEqual(self.run_main(get, changed), ("listed=false\n", None))
        # A name instead of a commit id could move between this run's reads.
        for name in ("BASE_SHA", "HEAD_SHA"):
            with self.subTest(name):
                self.assertEqual(self.run_main(api(one), 1, **{name: "main"}), ("listed=false\n", None))


if __name__ == "__main__":
    unittest.main()
