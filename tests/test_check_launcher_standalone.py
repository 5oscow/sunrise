import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.ci import check_launcher_standalone as guard


class LauncherStandalone(unittest.TestCase):
    def tree(self, files: dict) -> Path:
        root = Path(tempfile.mkdtemp())
        self.addCleanup(subprocess.run, ["rm", "-rf", str(root)])
        for name, text in files.items():
            (root / name).parent.mkdir(parents=True, exist_ok=True)
            (root / name).write_text(text)
        subprocess.run(["git", "init", "-q", str(root)], check=True)
        subprocess.run(["git", "-C", str(root), "add", "-A"], check=True)
        return root

    def found(self, files: dict) -> list:
        root = self.tree(files)
        return guard.problems(root, guard.tracked(root))

    def test_paths_inside_the_launcher_pass(self):
        self.assertEqual(self.found({
            "launcher/Cargo.toml": '[workspace]\nmembers = ["crates/a"]\n[dependencies]\na = { path = "crates/a" }\n',
            "launcher/crates/a/Cargo.toml": '[dependencies]\nb = { path = "../b" }\n',
            "launcher/crates/a/src/lib.rs": 'const L: &str = include_str!("../../../lang/en.json");\n'
                                            '#[path = "x_tests.rs"]\nmod tests;\n'
                                            'fn f() { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"); }\n',
            "outside.rs": 'include_str!("../../etc/passwd");\n',
        }), [])

    def test_every_way_out_is_named(self):
        found = self.found({
            "launcher/Cargo.toml": '[dependencies]\ngame = { path = "../crates/game" }\n'
                                   '[patch.crates-io]\nx = { path = "/opt/x" }\n',
            "launcher/src/main.rs": 'const V: &str = include_str!("../../tools/ci/rust-version");\n'
                                    '#[path = "../../crates/game/src/shared.rs"]\nmod shared;\n'
                                    'fn f() { Path::new(env!("CARGO_MANIFEST_DIR")).join(".."); }\n'
                                    'const W: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../README.md"));\n',
        })
        self.assertEqual(len(found), 6, found)
        self.assertIn("launcher/Cargo.toml: path '../crates/game' leaves launcher/", found)
        self.assertIn("launcher/src/main.rs:1: '../../tools/ci/rust-version' leaves launcher/", found)
        self.assertTrue(any(":4: '..' leaves" in problem for problem in found), found)

    def test_raw_strings_build_scripts_absolute_joins_and_windows_paths_are_read(self):
        found = self.found({
            "launcher/crates/a/Cargo.toml": '[dependencies]\ngame = { path = "..\\\\..\\\\..\\\\crates\\\\game" }\n',
            "launcher/crates/a/build.rs": 'fn main() {\n    let readme = std::fs::read_to_string("../../../README.md");\n'
                                          '    let ok = std::fs::read_to_string("assets/icon.png");\n'
                                          '    println!("cargo:rerun-if-changed=../../../tools/ci/rust-version");\n}\n',
            "launcher/crates/a/src/lib.rs": 'const R: &str = include_str!(r#"../../../../README.md"#);\n'
                                            'fn f() { Path::new(env!("CARGO_MANIFEST_DIR")).join("data").join("/etc/hostname"); }\n'
                                            'const W: &[u8] = include_bytes!("C:\\\\game\\\\icon.png");\n'
                                            'const B: &[u8] = include_bytes!(r"..\\..\\..\\..\\x.bin");\n',
        })
        self.assertEqual(len(found), 7, found)
        for expected in ["launcher/crates/a/Cargo.toml: path",
                         "launcher/crates/a/build.rs:2: '../../../README.md'",
                         "launcher/crates/a/build.rs:4: '../../../tools/ci/rust-version'",
                         "launcher/crates/a/src/lib.rs:1: '../../../../README.md'",
                         "launcher/crates/a/src/lib.rs:2: '/etc/hostname'",
                         "launcher/crates/a/src/lib.rs:3: 'C:\\\\game\\\\icon.png'",
                         "launcher/crates/a/src/lib.rs:4:"]:
            self.assertTrue(any(problem.startswith(expected) for problem in found), (expected, found))

    def test_build_script_chains_are_read_once_from_their_base_and_drive_relative_paths_leave(self):
        found = self.found({
            "launcher/Cargo.toml": "[package]\nname = 'l'\n",
            "launcher/build.rs": 'fn main() {\n'
                                 '    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("../rust-version");\n'
                                 '    Path::new("assets").join("icons").join("../../rust-version");\n'
                                 '    println!("cargo::rerun-if-changed=../tools/ci/rust-version");\n'
                                 '    Path::new(env!("CARGO_MANIFEST_DIR")).join("C:icon.png");\n'
                                 '    let up = PathBuf::from("assets").join("../../README.md");\n'
                                 '    let read = std::fs::read(std::path::PathBuf::new().join("../README.md"));\n'
                                 '    let here = PathBuf::new().join("assets").join("icon.png");\n}\n',
            "launcher/crates/b/Cargo.toml": "[package]\nname = 'b'\n",
            "launcher/crates/b/build.rs": 'fn main() {\n    println!("cargo:rerun-if-changed=..\\\\..\\\\..\\\\README.md");\n'
                                          '    println!("cargo:rerun-if-changed=src\\ncargo::rerun-if-changed=../../../x");\n'
                                          '    println!("cargo:rerun-if-changed=src\\\n        \\ncargo:rerun-if-changed=../../../y");\n'
                                          '    println!("cargo:rerun-if-changed=src\ncargo:rerun-if-changed=../../../z");\n}\n',
            "launcher/src/lib.rs": 'const I: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "\\\\assets\\\\icon.png"));\n',
        })
        self.assertEqual(sorted(problem.split(":")[1] for problem in found if "crates/b" not in problem),
                         ["4", "5", "6", "7"], found)
        self.assertTrue(any(p.startswith("launcher/crates/b/build.rs:2:") for p in found), found)
        # Two directives in one string, a newline apart: each path is its own.
        self.assertTrue(any(p.startswith("launcher/crates/b/build.rs:3: '../../../x'") for p in found), found)
        # A line continuation then `\\n`, and a newline inside the literal, each start the next directive.
        for line, path in [("4", "y"), ("6", "z")]:
            self.assertTrue(any(p.startswith(f"launcher/crates/b/build.rs:{line}: '../../../{path}'") for p in found), found)

    def test_this_repository_launcher_builds_from_its_own_files(self):
        self.assertEqual(guard.problems(guard.ROOT, guard.tracked(guard.ROOT)), [])


if __name__ == "__main__":
    unittest.main()
