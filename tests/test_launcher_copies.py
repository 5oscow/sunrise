"""The launcher's copies of game files (launcher/crates/, launcher/assets/) stay byte- and value-equal to the originals."""
import ast
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
COPIES = ROOT / "launcher/crates"

# (the game's file or folder, the launcher's copy)
SAME_BYTES = (
    ("crates/sunrise-release-sign/release-keys.txt", "release-verify/keys/sunrise-org-sunrise.txt"),
    ("crates/sunrise-release-sign/native-build-keys.txt", "release-verify/keys/native-builds.txt"),
    ("crates/sunrise-release-sign/tests/signed", "release-verify/tests/signed"),
    ("crates/sunrise-mod-package/tests/index", "smod/tests/index"),
    ("crates/sunrise-theme/tokens.css", "theme/tokens.css"),
    ("packaging/icons/hicolor/16x16/apps/sunrise.png", "../assets/icons/hicolor/16x16/apps/sunrise.png"),
    ("packaging/icons/hicolor/24x24/apps/sunrise.png", "../assets/icons/hicolor/24x24/apps/sunrise.png"),
    ("packaging/icons/hicolor/32x32/apps/sunrise.png", "../assets/icons/hicolor/32x32/apps/sunrise.png"),
    ("packaging/icons/hicolor/48x48/apps/sunrise.png", "../assets/icons/hicolor/48x48/apps/sunrise.png"),
    ("packaging/icons/hicolor/64x64/apps/sunrise.png", "../assets/icons/hicolor/64x64/apps/sunrise.png"),
    ("packaging/icons/hicolor/128x128/apps/sunrise.png", "../assets/icons/hicolor/128x128/apps/sunrise.png"),
    ("packaging/icons/hicolor/256x256/apps/sunrise.png", "../assets/icons/hicolor/256x256/apps/sunrise.png"),
    ("packaging/icons/hicolor/512x512/apps/sunrise.png", "../assets/icons/hicolor/512x512/apps/sunrise.png"),
    ("packaging/icons/hicolor/scalable/apps/sunrise.svg", "../assets/icons/hicolor/scalable/apps/sunrise.svg"),
    ("packaging/icons/sunrise.icns", "../assets/icons/sunrise.icns"),
    ("packaging/icons/sunrise.ico", "../assets/icons/sunrise.ico"),
    ("packaging/linux/sunrise.desktop", "../assets/linux/sunrise.desktop"),
    ("docs/media/village.webp", "../assets/media/village.webp"),
    ("docs/media/underwater.webp", "../assets/media/underwater.webp"),
    ("docs/media/nether.webp", "../assets/media/nether.webp"),
    ("LICENSE", "../assets/LICENSE"),
    ("LICENSES/MIT-Horizon.txt", "../assets/LICENSES/MIT-Horizon.txt"),
    ("crates/sunrise-prepare/tests/support/java_probe.rs", "../tests/support/java_probe.rs"),
    ("crates/sunrise-port-pack/src/zip_intake/source.rs", "../src/zip_intake/source.rs"),
    ("crates/sunrise-port-pack/src/zip_intake/tests.rs", "../src/zip_intake/tests.rs"),
)

# (the game's source, the launcher's copy, the edits that turn one into the other): compared as code, ignoring
# layout, module docs and `use` items, which the launcher's edition formats and orders its own way.
SAME_CODE = (
    ("crates/sunrise-port-pack/src/net.rs", "launcher/src/net.rs", (
        ("pub(crate) fn get_from_hosts", "pub fn get_from_hosts"),
        ('.set("User-Agent", crate::modrinth::AGENT)',
         '.set("User-Agent", concat!("sunrise-launcher/", env!("CARGO_PKG_VERSION")))'),
    )),
    ("crates/sunrise-port-pack/src/zip_intake.rs", "launcher/src/zip_intake.rs", (
        ("""if let Ok(directory) = zip64
            && (directory.entries, directory.size, directory.offset)
                == (classic.entries, classic.size, classic.offset)
        {
            return Ok((directory, Some(at)));
        }""", """if let Ok(directory) = zip64 {
            if (directory.entries, directory.size, directory.offset)
                == (classic.entries, classic.size, classic.offset)
            {
                return Ok((directory, Some(at)));
            }
        }"""),
    )),
)


def code(text: str) -> str:
    """Rust source as this test compares it: no module docs, `use` items, whitespace or trailing commas."""
    text = re.sub(r"^\s*//!.*$", "", text, flags=re.M)
    text = re.sub(r"^\s*(pub(\(crate\))? )?use [^;]*;", "", text, flags=re.M | re.S)
    return re.sub(r",([)\]}])", r"\1", re.sub(r"\s+", "", text))


def files(path: Path) -> dict:
    if path.is_file():
        return {"": path.read_bytes()}
    return {str(f.relative_to(path)): f.read_bytes() for f in sorted(path.rglob("*")) if f.is_file()}


def value(expression: str):
    """A Rust constant's value: a string literal, or integers joined by <<, *, + and -."""
    expression = expression.strip()
    if expression.startswith('"'):
        return ast.literal_eval(expression)
    try:
        tree = ast.parse(re.sub(r"(\d)_(?=\d)", r"\1", expression), mode="eval").body
    except SyntaxError as error:
        raise ValueError(f"not a constant this test reads: {expression}") from error

    def evaluate(node):
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        if isinstance(node, ast.BinOp):
            ops = {ast.LShift: lambda a, b: a << b, ast.Mult: lambda a, b: a * b,
                   ast.Add: lambda a, b: a + b, ast.Sub: lambda a, b: a - b}
            return ops[type(node.op)](evaluate(node.left), evaluate(node.right))
        raise ValueError(f"not a constant this test reads: {expression}")

    return evaluate(tree)


def constant(path: str, name: str):
    source = (ROOT / path).read_text(encoding="utf-8")
    found = re.search(rf"\bconst {name}\s*:\s*[^=]+=\s*(.+?);", source, re.S)
    if not found:
        raise AssertionError(f"{path} has no const {name}")
    return value(found.group(1))


class LauncherCopies(unittest.TestCase):
    def test_copied_files_are_the_games_byte_for_byte(self):
        for game, copy in SAME_BYTES:
            with self.subTest(copy=copy):
                original, copied = files(ROOT / game), files(COPIES / copy)
                self.assertTrue(original, f"{game} is empty or missing")
                self.assertEqual(sorted(copied), sorted(original), f"{copy} lists other files than {game}")
                for name in original:
                    where = f"{name} in " if name else ""
                    self.assertEqual(copied[name], original[name], f"{where}{copy} differs from {game}")

    def test_rewritten_copies_are_the_games_code_with_their_listed_edits(self):
        for game, copy, edits in SAME_CODE:
            with self.subTest(copy=copy):
                original = code((ROOT / game).read_text(encoding="utf-8"))
                for old, new in edits:
                    self.assertIn(code(old), original, f"{game} no longer has the code {copy} edits: update both")
                    original = original.replace(code(old), code(new))
                self.assertEqual(code((ROOT / copy).read_text(encoding="utf-8")), original,
                                 f"{copy} differs from {game} beyond its listed edits: copy the change, or list it")

    def test_format_constants_match_the_games(self):
        package_format = int((ROOT / "crates/sunrise-mod-package/src/sdk/package-format").read_text().strip())
        self.assertEqual(constant("launcher/crates/smod/src/index/native.rs", "PACKAGE_FORMAT"), package_format)
        self.assertEqual(constant("launcher/crates/smod/src/manifest.rs", "USES_FILE"),
                         constant("crates/sunrise-mod-surface/src/uses.rs", "USES_FILE"))
        self.assertEqual(constant("launcher/crates/smod/src/manifest.rs", "MAX_USES"),
                         constant("crates/sunrise-mod-surface/src/uses.rs", "MAX_USES_BYTES"))

    def test_a_drifted_copy_is_caught(self):
        self.assertEqual(code("//! Doc.\nuse a::{b,\n c};\nfn f() { g( 1 ) }\n"), "fnf(){g(1)}")
        self.assertNotEqual(code("fn f() { g(1) }"), code("fn f() { g(2) }"))
        self.assertEqual(value("4 << 20"), 4 * 1024 * 1024)
        self.assertEqual(value("1_000"), 1000)
        self.assertEqual(value('"uses.json"'), "uses.json")
        with self.assertRaises(ValueError):
            value("mod_surface::uses::MAX_USES_BYTES as u64")


if __name__ == "__main__":
    unittest.main()
