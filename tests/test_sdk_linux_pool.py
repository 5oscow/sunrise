"""Run the SDK routing and dependency setup boundary without downloading or building."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from tests.test_ci_concurrency import ROOT, yaml


@unittest.skipUnless(sys.platform == "linux" and shutil.which("bash") and shutil.which("jq"),
                     "requires runner's Bash and jq")
class SdkLinuxPoolTests(unittest.TestCase):
    def test_only_opted_in_linux_legs_with_a_pool_route_move(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/prebuilt-game-build.yml").read_text())
        step = next(step for step in workflow["jobs"]["fresh"]["steps"] if step.get("id") == "legs")
        original = {entry["name"]: entry["os"] for entry in json.loads(step["env"]["ALL"])}
        self.assertEqual(original, {"linux": "ubuntu-22.04", "linux-server": "ubuntu-22.04",
                                    "windows": "windows-latest", "macos": "macos-15"})
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "matrix"
            for opt_in, route in (("", "true"), ("false", "true"), ("true", ""), ("true", "false"), ("true", "true")):
                for legs in ("all", "linux", "windows-macos"):
                    with self.subTest(opt_in=opt_in, route=route, legs=legs):
                        output.write_text("")
                        env = dict(os.environ, ALL=step["env"]["ALL"], GITHUB_OUTPUT=str(output),
                                   LEGS=legs, POOL_ENABLED=opt_in, SELF_HOSTED=route)
                        subprocess.run(["bash", "-eu", "-c", step["run"]], env=env, check=True, timeout=10)
                        rows = json.loads(output.read_text().removeprefix("matrix="))["include"]
                        expected = {name: "dyna-large" if name.startswith("linux") and opt_in == route == "true" else os_name
                                    for name, os_name in original.items()
                                    if legs == "all" or (legs == "linux") == name.startswith("linux")}
                        self.assertEqual({row["name"]: row["os"] for row in rows}, expected)

    def test_public_route_timestamp_preserves_contents_only_callers(self):
        # With GitHub's scoped token both gh lookups are denied. Only the explicit public fallback may read
        # the creation time. The lease was valid then, but has expired by the time the route step runs.
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for name, body in {"gh": "exit 1", "curl": "printf '%s\\n' '{\"created_at\":\"1970-01-01T00:15:00Z\"}'"}.items():
                tool = root / name
                tool.write_text(f"#!/bin/sh\n{body}\n")
                tool.chmod(0o755)
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}", VARS_ENABLED="true", VARS_LEASE="950",
                       NOW="1000", EVENT="workflow_dispatch", SAME_REPO="true", GITHUB_REPOSITORY="sunrise-org/sunrise",
                       GITHUB_RUN_ID="1", GITHUB_API_URL="https://api.github.com", GITHUB_OUTPUT=str(root / "output"))
            script = ROOT / "tools/ci/dyna_ci_route.sh"
            for fallback, expected in (("true", "true"), ("", "false")):
                result = subprocess.run(["bash", script], env=dict(env, ALLOW_PUBLIC_RUN_LOOKUP=fallback),
                                        check=True, text=True, capture_output=True, timeout=10)
                self.assertEqual(result.stdout.strip(), f"self_hosted={expected}")

    def test_setup_rejects_corrupt_cached_and_downloaded_bytes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            build = root / "build"
            archive = (build / "sdk-glibc-2.35-zig-0.15.2-jammy-20261118T044000Z"
                       / "downloads/zig-x86_64-linux-0.15.2.tar.xz")
            archive.parent.mkdir(parents=True)
            fake = root / "not-zig.tar.xz"
            fake.write_bytes(b"wrong bytes")
            curl = root / "curl"
            curl.write_text('''#!/bin/sh
printf 'downloaded\\n' >> "$CALLS"
while [ "$#" -gt 0 ]; do
    if [ "$1" = --output ]; then exec cp -- "$CURL_FIXTURE" "$2"; fi
    shift
done
exit 1
''')
            curl.chmod(0o755)
            env_file, path_file, calls = (root / name for name in ("env", "path", "calls"))
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}", DYNA_CI_BUILD=str(build),
                       GITHUB_ENV=str(env_file), GITHUB_PATH=str(path_file), CURL_FIXTURE=str(fake), CALLS=str(calls))
            for cached in (True, False):
                with self.subTest(cached=cached):
                    if cached:
                        shutil.copyfile(fake, archive)
                    result = subprocess.run(["bash", ROOT / "tools/ci/glibc_floor_setup.sh"], env=env,
                                            capture_output=True, text=True, timeout=10)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(f"checksum mismatch for {archive.name}", result.stderr)
                    self.assertEqual(archive.exists(), cached)
                    self.assertEqual(calls.exists(), not cached)
                    self.assertFalse(env_file.exists())
                    self.assertFalse(path_file.exists())
                    if cached:
                        archive.unlink()
