#!/usr/bin/env python3
"""verify-static's guard of the launcher split: nothing a `launcher/` build reads lies outside `launcher/`."""

import re
import subprocess
import sys
import tomllib
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[2]
HOME = "launcher"

STRING = r'"((?:[^"\\]|\\.)*)"'
RAW = re.compile(r'(?<![A-Za-z0-9_])b?r(#*)"(.*?)"\1', re.S)
INCLUDE = re.compile(r'\b(include_str|include_bytes|include)!\s*\(\s*' + STRING)
PATH_ATTR = re.compile(r'#\[\s*path\s*=\s*' + STRING + r'\s*\]')
MANIFEST_JOIN = re.compile(r'env!\(\s*"CARGO_MANIFEST_DIR"\s*\)\s*\)?((?:\s*\.join\(\s*' + STRING + r'\s*\))+)')
MANIFEST_CONCAT = re.compile(r'concat!\(\s*env!\(\s*"CARGO_MANIFEST_DIR"\s*\)\s*,\s*' + STRING)
JOINED = re.compile(r'\.join\(\s*' + STRING + r'\s*\)')
# A build script runs in its crate's folder: a literal it reads or opens, reruns on, or starts a path from (joined in turn) is a path from there.
BUILD_READ = re.compile(r'\b(?:read_to_string|read|read_dir|copy|open)\(\s*' + STRING)
LITERAL = re.compile(STRING, re.S)
RERUN = re.compile(r'^\s*cargo::?rerun-if-changed=(.+)$', re.M)
BUILD_PATH = re.compile(r'\b(?:Path::new|PathBuf::from)\(\s*' + STRING + r'\s*\)((?:\s*\.join\(\s*' + STRING + r'\s*\))*)')
BUILD_EMPTY = re.compile(r'\b(?:PathBuf::new|PathBuf::default)\(\s*\)((?:\s*\.join\(\s*' + STRING + r'\s*\))+)')
ABSOLUTE = re.compile(r'^(?:/|[A-Za-z]:|//)')


def tracked(root: Path) -> list[str]:
    """The tracked files under `launcher/`, as repository paths."""
    listed = subprocess.run(["git", "-C", str(root), "ls-files", "-z", "--", HOME], capture_output=True, check=True)
    return [p for p in listed.stdout.decode().split("\0") if p]


def plain(text: str) -> str:
    """`text` with its raw string literals written as plain ones, on the same lines."""
    return RAW.sub(lambda m: '"' + m.group(2).replace("\\", "\\\\").replace('"', '\\"') + '"', text)


def unescaped(literal: str) -> str:
    """A plain string literal's text."""
    return re.sub(r"\\(.)", r"\1", literal)


def rust_text(literal: str) -> str:
    """A plain string literal's text as Rust reads it: a line continuation drops the newline and the indent after it, `\\n` is a newline."""
    literal = re.sub(r"\\\n\s*", "", literal)
    return re.sub(r"\\(.)", lambda m: {"n": "\n", "r": "\r", "t": "\t"}.get(m.group(1), m.group(1)), literal, flags=re.S)


def joined(parts: list[str]) -> str:
    """`Path::join` of `parts` in turn: an absolute part replaces what came before, as in Rust."""
    path = ""
    for part in parts:
        part = unescaped(part).replace("\\", "/")
        path = part if ABSOLUTE.match(part) or not path else f"{path}/{part}"
    return path or "."


def inside(base: PurePosixPath, relative: str) -> bool:
    """Whether `relative` (`/` or `\\` separated), read from the folder `base`, stays in `launcher/`."""
    relative = relative.replace("\\", "/")
    if ABSOLUTE.match(relative):
        return False
    parts: list[str] = []
    for part in (base / relative).parts:
        if part == "..":
            if not parts:
                return False
            parts.pop()
        elif part != ".":
            parts.append(part)
    return bool(parts) and parts[0] == HOME


def manifest_paths(text: str) -> list[str]:
    """Every path a manifest names: `path` and `build` values in any table, and workspace members and excludes."""
    found: list[str] = []

    def walk(value, key=None):
        if isinstance(value, dict):
            for name, inner in value.items():
                if name in ("members", "exclude", "default-members") and isinstance(inner, list):
                    found.extend(m for m in inner if isinstance(m, str))
                else:
                    walk(inner, name)
        elif isinstance(value, list):
            for inner in value:
                walk(inner, key)
        elif key in ("path", "build") and isinstance(value, str):
            found.append(value)

    walk(tomllib.loads(text))
    return found


def crate_dir(path: PurePosixPath, manifests: set[PurePosixPath]) -> PurePosixPath:
    """The folder of the nearest manifest above `path`: what `CARGO_MANIFEST_DIR` is for it."""
    for folder in path.parents:
        if folder / "Cargo.toml" in manifests:
            return folder
    return PurePosixPath(HOME)


def problems(root: Path, files: list[str]) -> list[str]:
    """`file:line: what` for every path that leaves `launcher/`."""
    found = []
    manifests = {PurePosixPath(f) for f in files if f.endswith("Cargo.toml")}
    for name in files:
        path = PurePosixPath(name)
        if path.name == "Cargo.toml":
            text = (root / name).read_text(encoding="utf-8")
            for value in manifest_paths(text):
                if not inside(path.parent, value):
                    found.append(f"{name}: path {value!r} leaves {HOME}/")
        elif path.suffix == ".rs":
            text = plain((root / name).read_text(encoding="utf-8", errors="replace"))
            here = crate_dir(path, manifests)
            checks = [(m, path.parent, unescaped(m.group(2))) for m in INCLUDE.finditer(text)]
            checks += [(m, path.parent, unescaped(m.group(1))) for m in PATH_ATTR.finditer(text)]
            checks += [(m, here, unescaped(m.group(1)).replace("\\", "/").lstrip("/")) for m in MANIFEST_CONCAT.finditer(text)]
            checks += [(m, here, joined(JOINED.findall(m.group(1)))) for m in MANIFEST_JOIN.finditer(text)]
            if path.name == "build.rs":
                checks += [(m, here, unescaped(m.group(1))) for m in BUILD_READ.finditer(text)]
                # What a build script prints, line by line as Cargo reads it: each rerun-if-changed path is its own.
                checks += [
                    (m, here, rerun.group(1).strip())
                    for m in LITERAL.finditer(text)
                    if "rerun-if-changed" in m.group(1)
                    for rerun in RERUN.finditer(rust_text(m.group(1)))
                ]
                checks += [(m, here, joined([m.group(1), *JOINED.findall(m.group(2))])) for m in BUILD_PATH.finditer(text)]
                checks += [(m, here, joined(JOINED.findall(m.group(1)))) for m in BUILD_EMPTY.finditer(text)]
            for match, base, value in checks:
                if not inside(base, value):
                    line = text.count("\n", 0, match.start()) + 1
                    found.append(f"{name}:{line}: {value!r} leaves {HOME}/")
    return found


def main(argv: list[str]) -> int:
    root = Path(argv[1]) if len(argv) > 1 else ROOT
    found = problems(root, tracked(root))
    for problem in found:
        print(problem)
    if found:
        print(f"{len(found)} place(s) where a launcher build reads outside {HOME}/: the launcher repository won't have them")
        return 1
    print(f"{HOME}/ builds from its own files")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
