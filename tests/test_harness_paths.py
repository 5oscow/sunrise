"""Every repository path a parity harness names exists.

A harness hashes its sources to key its cached results, and the hash fails on the first missing file, before the
harness compares anything: eleven harnesses named files the game/ area moves had left behind (2026-11-16).
"""
import ast
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
# Where a harness's sources live; build output (target/, .local/) is made by the run.
SOURCES = ("crates/", "mods/", "tools/", "docs/", "launcher/", "xtask/")


class HarnessPaths(unittest.TestCase):
    def test_every_path_a_harness_names_exists(self):
        harnesses = sorted((ROOT / "tools").glob("test_*_parity.py"))
        self.assertTrue(harnesses)
        for harness in harnesses:
            for node in ast.walk(ast.parse(harness.read_text(encoding="utf-8"))):
                if not (isinstance(node, ast.Constant) and isinstance(node.value, str)):
                    continue
                path = node.value
                # A glob or template names no single file.
                if not path.startswith(SOURCES) or any(c in path for c in "*?{}<> \n"):
                    continue
                with self.subTest(harness=harness.name, line=node.lineno):
                    self.assertTrue((ROOT / path).exists(), f"{harness.name}:{node.lineno} names {path}")


if __name__ == "__main__":
    unittest.main()
