#!/usr/bin/env python3
"""Pins `sunrise-verify` in the server install scripts.

    pin_verifier.py verifier@X.Y.Z [--repository OWNER/NAME] [--from-dir FOLDER]

Downloads the release's SHA256SUMS, checks that it lists exactly the six binaries `verifier.yml` publishes, downloads
each binary and checks it against the sums, then rewrites the one marked pin block in packaging/server/install.sh and
install.ps1 (the version and one SHA-256 per platform). The scripts take a downloaded verifier only if it matches
that pin, so the pin on main is the trust anchor. The change goes in an ordinary pull request.
`--from-dir` reads the files from a folder instead of the release (a release downloaded by hand).
"""
import argparse
import hashlib
import re
import sys
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = (ROOT / "packaging/server/install.sh", ROOT / "packaging/server/install.ps1")
REPOSITORY = "sunrise-org/sunrise"
TAG = re.compile(r"verifier@(\d+\.\d+\.\d+)")
# platform -> the release asset; the names verifier.yml publishes.
PLATFORMS = {
    "linux-x86_64": "sunrise-verify-linux-x86_64",
    "linux-aarch64": "sunrise-verify-linux-aarch64",
    "macos-arm64": "sunrise-verify-macos-arm64",
    "macos-x64": "sunrise-verify-macos-x64",
    "windows-x64": "sunrise-verify-windows-x64.exe",
    "windows-arm64": "sunrise-verify-windows-arm64.exe",
}
BEGIN = "# >>> verifier pin"
END = "# <<< verifier pin"
BLOCK = re.compile(rf"^{re.escape(BEGIN)}.*?^{re.escape(END)}[^\n]*\n", re.S | re.M)
NOTE = "tools/ci/pin_verifier.py rewrites this block; do not edit it by hand"


def block(script: Path, version: str, hashes: dict[str, str]) -> str:
    """The pin block as `script`'s language writes it."""
    if script.suffix == ".ps1":
        rows = "".join(f"    '{platform}' = '{hashes[platform]}'\n" for platform in PLATFORMS)
        body = f"$VerifierVersion = '{version}'\n$VerifierSha256 = @{{\n{rows}}}\n"
    else:
        rows = "".join(f"VERIFIER_SHA256_{platform.replace('-', '_')}={hashes[platform]}\n" for platform in PLATFORMS)
        body = f"VERIFIER_VERSION={version}\n{rows}"
    return f"{BEGIN} ({NOTE})\n{body}{END}\n"


def pinned(script: Path) -> tuple[str, dict[str, str]]:
    """The version and per-platform hashes a script's pin block holds."""
    found = BLOCK.search(script.read_text(encoding="utf-8"))
    if not found:
        sys.exit(f"{script}: no verifier pin block")
    text = found.group(0)
    if script.suffix == ".ps1":
        version = re.search(r"\$VerifierVersion = '([^']*)'", text).group(1)
        hashes = dict(re.findall(r"'([a-z0-9_-]+)' = '([^']*)'", text))
    else:
        version = re.search(r"^VERIFIER_VERSION=(\S*)$", text, re.M).group(1)
        names = {platform.replace("-", "_"): platform for platform in PLATFORMS}
        hashes = {names[name.removeprefix("VERIFIER_SHA256_")]: value
                  for name, value in re.findall(r"^(VERIFIER_SHA256_\w+)=(\S*)$", text, re.M)}
    return version, hashes


def rewrite(script: Path, version: str, hashes: dict[str, str]) -> None:
    text = script.read_text(encoding="utf-8")
    if not BLOCK.search(text):
        sys.exit(f"{script}: no verifier pin block")
    script.write_text(BLOCK.sub(lambda _: block(script, version, hashes), text, count=1), encoding="utf-8", newline="\n")


def parse_sums(text: str) -> dict[str, str]:
    sums = {}
    for line in text.splitlines():
        if line.strip():
            digest, name = line.split(None, 1)
            sums[name.strip().removeprefix("*")] = digest
    return sums


def fetch(repository: str, tag: str, name: str, folder: Path | None) -> bytes:
    if folder is not None:
        return (folder / name).read_bytes()
    url = f"https://github.com/{repository}/releases/download/{urllib.parse.quote(tag)}/{urllib.parse.quote(name)}"
    with urllib.request.urlopen(url, timeout=120) as response:
        return response.read()


def pin(tag: str, repository: str, folder: Path | None) -> None:
    version = TAG.fullmatch(tag)
    if not version:
        sys.exit(f"{tag}: not a verifier release tag (verifier@X.Y.Z)")
    sums = parse_sums(fetch(repository, tag, "SHA256SUMS", folder).decode())
    if set(sums) != set(PLATFORMS.values()):
        sys.exit(f"{tag}'s SHA256SUMS lists {sorted(sums)}, expected exactly {sorted(PLATFORMS.values())}")
    hashes = {}
    for platform, name in PLATFORMS.items():
        got = hashlib.sha256(fetch(repository, tag, name, folder)).hexdigest()
        if got != sums[name]:
            sys.exit(f"{name} is {got}, but {tag}'s SHA256SUMS says {sums[name]}: nothing pinned")
        hashes[platform] = got
    for script in SCRIPTS:
        rewrite(script, version.group(1), hashes)
        print(f"{script.name}: pinned verifier {version.group(1)}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("tag")
    parser.add_argument("--repository", default=REPOSITORY)
    parser.add_argument("--from-dir", type=Path)
    args = parser.parse_args()
    pin(args.tag, args.repository, args.from_dir)
