"""The install scripts pin one `sunrise-verify`: the same version and the same SHA-256 per platform.

PURPOSE: install.sh and install.ps1 each carry the pin block tools/ci/pin_verifier.py rewrites; a hand edit of one
that leaves the other behind would make a platform check against a hash the other script doesn't trust.
REFS: tools/ci/pin_verifier.py, packaging/server/install.sh, packaging/server/install.ps1.
Headless, no network.
"""

import hashlib
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools" / "ci"))
import pin_verifier  # noqa: E402

SH, PS1 = pin_verifier.SCRIPTS


class VerifierPins(unittest.TestCase):
    def test_both_scripts_pin_the_same_version_and_hashes(self):
        sh, ps1 = pin_verifier.pinned(SH), pin_verifier.pinned(PS1)
        self.assertEqual(sh, ps1)
        self.assertEqual(set(sh[1]), set(pin_verifier.PLATFORMS))
        # A real pin is a version and 64-hex hashes; until the first release both are the same placeholder, which
        # makes the scripts refuse (they take nothing "unpinned").
        if sh[0] != "unpinned":
            self.assertRegex(sh[0], r"^\d+\.\d+\.\d+$")
            for platform, digest in sh[1].items():
                self.assertRegex(digest, r"^[0-9a-f]{64}$", platform)
        else:
            self.assertEqual(set(sh[1].values()), {"unpinned"})

    def test_pinning_a_release_rewrites_only_the_block_in_both_scripts(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            binaries = {name: f"binary of {name}".encode() for name in pin_verifier.PLATFORMS.values()}
            for name, data in binaries.items():
                (folder / name).write_bytes(data)
            sums = "".join(f"{hashlib.sha256(data).hexdigest()}  {name}\n" for name, data in binaries.items())
            (folder / "SHA256SUMS").write_text(sums)
            copies = []
            for script in pin_verifier.SCRIPTS:
                copy = Path(tmp) / script.name
                shutil.copy(script, copy)
                copies.append(copy)
            saved = pin_verifier.SCRIPTS
            pin_verifier.SCRIPTS = tuple(copies)
            try:
                pin_verifier.pin("verifier@2.3.4", pin_verifier.REPOSITORY, folder)
                for copy, original in zip(copies, saved):
                    version, hashes = pin_verifier.pinned(copy)
                    self.assertEqual(version, "2.3.4")
                    self.assertEqual(hashes["windows-arm64"],
                                     hashlib.sha256(binaries["sunrise-verify-windows-arm64.exe"]).hexdigest())
                    cut = pin_verifier.BLOCK
                    self.assertEqual(cut.sub("", copy.read_text()), cut.sub("", original.read_text()))
                # One binary that is not what SHA256SUMS lists: nothing is rewritten.
                (folder / "sunrise-verify-linux-x86_64").write_bytes(b"swapped")
                before = [copy.read_text() for copy in copies]
                with self.assertRaises(SystemExit) as refused:
                    pin_verifier.pin("verifier@2.3.5", pin_verifier.REPOSITORY, folder)
                self.assertIn("nothing pinned", str(refused.exception))
                self.assertEqual(before, [copy.read_text() for copy in copies])
            finally:
                pin_verifier.SCRIPTS = saved


if __name__ == "__main__":
    unittest.main()
