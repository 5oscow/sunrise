"""tools/book_quickstart.py: which blocks of a page run, and how a failing one is named."""
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import book_quickstart  # noqa: E402

PAGE = """# A page

```sh
echo one > one.txt
cd "$(mktemp -d -p .)"
```

```text
not a command
```

<!-- quickstart-test: skip, needs the game -->
```sh
exit 9
```

```sh
test -f ../one.txt
false
```

```sh
echo never > never.txt
```
"""


class BookQuickstart(unittest.TestCase):
    def test_sh_blocks_are_found_with_their_line_and_skip_reason(self):
        self.assertEqual(
            [(line, skip) for line, _, skip in book_quickstart.blocks(PAGE)],
            [(3, None), (13, "needs the game"), (17, None), (22, None)])

    def test_blocks_share_one_shell_and_the_first_failing_block_is_named(self):
        with tempfile.TemporaryDirectory() as folder:
            page = Path(folder, "page.md")
            page.write_text(PAGE)
            log = open(Path(folder, "log"), "w+")
            # The third block sees the first's `cd`, fails on its own second line, and the fourth never runs.
            self.assertEqual(book_quickstart.run_page(page, folder, dict(os.environ), log), 17)
            self.assertTrue(Path(folder, "one.txt").exists())
            self.assertEqual(list(Path(folder).rglob("never.txt")), [])
            log.close()


if __name__ == "__main__":
    unittest.main()
