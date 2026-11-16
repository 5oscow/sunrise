"""A push to main starts the day's real daily only when it is due and none has started."""
import datetime as dt
from pathlib import Path
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools/ci"))
import daily_catch_up  # noqa: E402

UTC = dt.timezone.utc


def run(title, branch="main", conclusion=None):
    return {"head_branch": branch, "display_title": title, "conclusion": conclusion, "html_url": f"run/{title}"}


class CatchUpTests(unittest.TestCase):
    def catch_up(self, now, listings):
        """main() at now, with the API listing listings in turn; returns (status, dispatches, queried created filters)."""
        dispatched, queried = [], []
        listings = iter(listings)

        def api(path, body=None):
            if body is not None:
                dispatched.append((path, body))
                return None
            queried.append(path)
            return {"workflow_runs": next(listings)}

        with mock.patch.object(daily_catch_up, "api", api), mock.patch.object(daily_catch_up.time, "sleep"):
            status = daily_catch_up.main(now)
        return status, dispatched, queried

    def test_before_the_daily_hour_nothing_is_asked(self):
        status, dispatched, queried = self.catch_up(dt.datetime(2026, 11, 16, 8, 59, tzinfo=UTC), [])
        self.assertEqual((status, dispatched, queried), (0, [], []))

    def test_a_real_run_since_the_hour_is_enough(self):
        for started in [run("daily abc"), run("daily abc", conclusion="failure"), run("daily abc", conclusion="success")]:
            with self.subTest(started=started):
                status, dispatched, queried = self.catch_up(dt.datetime(2026, 11, 16, 15, 0, tzinfo=UTC), [[started]])
                self.assertEqual((status, dispatched), (0, []))
                self.assertIn("created=%3E%3D2026-11-16T09%3A00%3A00Z", queried[0])

    def test_no_real_run_dispatches_as_the_schedule_would_and_waits_for_it(self):
        # A dry run, another branch's run and a cancelled real run do not count.
        none = [run("daily abc (dry run)"), run("daily abc", branch="topic"), run("daily abc", conclusion="cancelled")]
        status, dispatched, queried = self.catch_up(dt.datetime(2026, 11, 16, 9, 0, tzinfo=UTC),
                                                     [none, none, none + [run("daily def")]])
        self.assertEqual(status, 0)
        self.assertEqual(dispatched, [("actions/workflows/daily.yml/dispatches",
                                       {"ref": "main", "inputs": {"dry_run": "false", "scheduled": "true"}})])
        self.assertEqual(len(queried), 3)

    def test_a_dispatch_that_never_shows_fails_the_job(self):
        status, dispatched, _ = self.catch_up(dt.datetime(2026, 11, 16, 23, 0, tzinfo=UTC), [[]] * 13)
        self.assertEqual((status, len(dispatched)), (1, 1))


if __name__ == "__main__":
    unittest.main()
