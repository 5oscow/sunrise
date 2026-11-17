import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools" / "ci"))
import cpu_use  # noqa: E402


class ReportTests(unittest.TestCase):
    def test_use_and_throttling_are_measured_against_the_quota_since_the_mark(self):
        before = {"usage_usec": 10_000_000, "nr_periods": 100, "nr_throttled": 5, "throttled_usec": 1_000_000,
                  "wall_usec": 0}
        after = {"usage_usec": 410_000_000, "nr_periods": 1100, "nr_throttled": 305, "throttled_usec": 61_000_000,
                 "wall_usec": 100_000_000}
        self.assertEqual(cpu_use.report(before, after, 8.0),
                         "400 CPU-s in 100 s on 8 CPUs: 50% of the quota, throttled 60 s in 300 of 1000 periods")


if __name__ == "__main__":
    unittest.main()
