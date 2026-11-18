"""Exercise the verify-static guard with synthetic Cargo trees, without running Cargo."""

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest.mock import patch

from tools.ci import check_extras_features

ROOT = check_extras_features.ROOT
GAME = f"sunrise-game v0.1.0 ({ROOT}/crates/sunrise-game)|audio\n"


def tree(*lines):
    """`cargo tree --charset ascii` output under the game: each line is (depth, text)."""
    return GAME + "".join(f"{'|   ' * (depth - 1)}|-- {text}\n" for depth, text in lines)


SHARED = tree((1, "shared v1.0.0|std"))


class ExtrasFeaturesTest(unittest.TestCase):
    def check(self, default, extras_off, *, root="sunrise-game"):
        def cargo_tree(command, **kwargs):
            if command[command.index("-p") + 1] != root:
                return SHARED
            return extras_off if "--no-default-features" in command else default

        with patch.object(check_extras_features.subprocess, "check_output", side_effect=cargo_tree) as cargo, \
                redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()) as errors:
            status = check_extras_features.main()
        self.assertEqual(cargo.call_count, 4)
        return status, errors.getvalue()

    def test_equal_feature_sets_pass(self):
        self.assertEqual(self.check(tree((1, "shared v1.0.0|alloc,std"), (1, "shared v1.0.0|alloc,std (*)")),
                                    tree((1, "shared v1.0.0|std,alloc"))), (0, ""))

    def test_a_shared_package_with_different_features_fails_in_either_graph(self):
        for root in ("sunrise-game", "sunrise-mod-host"):
            with self.subTest(root=root):
                status, diagnostic = self.check(SHARED, tree((1, "shared v1.0.0|")), root=root)
                self.assertEqual(status, 1)
                self.assertIn(f"{root}: shared v1.0.0 (target): default [std]; Extras-off []", diagnostic)

    def test_one_sided_packages_pass(self):
        self.assertEqual(self.check(tree((1, "shared v1.0.0|std"), (1, "sunrise-friends v0.1.0|")),
                                    tree((1, "shared v1.0.0|std"), (1, "off-only v0.2.0|"))), (0, ""))

    def test_workspace_crates_own_the_extras_features_and_may_differ(self):
        default = tree((1, f"sunrise-client-lib v0.1.0 ({ROOT}/crates/sunrise-client-lib)|audio,controllers"))
        extras_off = tree((1, f"sunrise-client-lib v0.1.0 ({ROOT}/crates/sunrise-client-lib)|audio"))
        self.assertEqual(self.check(default, extras_off), (0, ""))

    def test_a_path_dependency_outside_the_workspace_is_still_compared(self):
        for path in ("/elsewhere/outside", f"{ROOT}-sibling/outside"):
            with self.subTest(path=path):
                status, diagnostic = self.check(tree((1, f"outside v0.1.0 ({path})|std")),
                                                tree((1, f"outside v0.1.0 ({path})|")))
                self.assertEqual(status, 1)
                self.assertIn(f"outside v0.1.0 ({path}) (target): default [std]; Extras-off []", diagnostic)

    def test_the_host_features_of_a_package_do_not_cover_its_target_features(self):
        # The default build's target libc has extra_traits; the vanilla build's target libc matches only the host's.
        default = (GAME + "|-- nix v0.1.0|\n|   `-- libc v0.2.0|extra_traits,std\n"
                   + "`-- uses-libc v0.1.0|\n    [build-dependencies]\n    `-- libc v0.2.0|std\n")
        extras_off = GAME + "|-- libc v0.2.0|std\n`-- uses-libc v0.1.0|\n    [build-dependencies]\n    `-- libc v0.2.0|std\n"
        status, diagnostic = self.check(default, extras_off)
        self.assertEqual(status, 1)
        self.assertIn("libc v0.2.0 (target): default [extra_traits, std]; Extras-off [std]", diagnostic)
        self.assertNotIn("(host)", diagnostic)

    def test_build_dependencies_and_proc_macros_are_host_and_their_siblings_after_them_are_not(self):
        default = (GAME + "|-- a v0.1.0|\n|   [build-dependencies]\n|   `-- cc v1.0.0|parallel\n"
                   + "|-- derive v0.1.0 (proc-macro)|\n|   `-- syn v2.0.0|full\n|-- syn v2.0.0|\n")
        extras_off = (GAME + "|-- a v0.1.0|\n|   [build-dependencies]\n|   `-- cc v1.0.0|\n"
                      + "|-- derive v0.1.0 (proc-macro)|\n|   `-- syn v2.0.0|\n|-- syn v2.0.0|\n")
        status, diagnostic = self.check(default, extras_off)
        self.assertEqual(status, 1)
        self.assertIn("cc v1.0.0 (host): default [parallel]; Extras-off []", diagnostic)
        self.assertIn("syn v2.0.0 (host): default [full]; Extras-off []", diagnostic)
        self.assertNotIn("(target)", diagnostic)


if __name__ == "__main__":
    unittest.main()
