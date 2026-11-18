"""Exercise the verify-static guard with synthetic Cargo trees, without running Cargo."""

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest.mock import patch

from tools.ci import check_extras_features

SHARED = "shared v1.0.0|std\n"


class ExtrasFeaturesTest(unittest.TestCase):
    def check(self, default, extras_off, *, root="sunrise-game"):
        def tree(command, **kwargs):
            if command[command.index("-p") + 1] != root:
                return SHARED
            return extras_off if "--no-default-features" in command else default

        with patch.object(check_extras_features.subprocess, "check_output", side_effect=tree) as cargo, \
                redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()) as errors:
            status = check_extras_features.main()
        self.assertEqual(cargo.call_count, 4)
        return status, errors.getvalue()

    def test_equal_feature_sets_pass(self):
        self.assertEqual(self.check("shared v1.0.0|alloc,std\nshared v1.0.0|alloc,std (*)\n",
                                    "shared v1.0.0|std,alloc\n"), (0, ""))

    def test_a_shared_package_with_different_features_fails_in_either_graph(self):
        for root in ("sunrise-game", "sunrise-mod-host"):
            with self.subTest(root=root):
                status, diagnostic = self.check(SHARED, "shared v1.0.0|\n", root=root)
                self.assertEqual(status, 1)
                self.assertIn(f"{root}: shared v1.0.0: default [std]; Extras-off []", diagnostic)

    def test_one_sided_packages_pass(self):
        self.assertEqual(self.check(SHARED + "sunrise-friends v0.1.0|\n",
                                    SHARED + "off-only v0.2.0|\n"), (0, ""))

    def test_extras_owners_may_differ(self):
        owners = ("sunrise-client", "sunrise-game", "sunrise-mod-host")
        default = "".join(f"{name} v0.1.0 (/workspace/{name})|audio,sunrise-extras\n" for name in owners)
        extras_off = "".join(f"{name} v0.1.0 (/workspace/{name})|audio\n" for name in owners)
        self.assertEqual(self.check(SHARED + default, SHARED + extras_off), (0, ""))

    def test_an_extra_default_host_or_target_instance_passes(self):
        self.assertEqual(self.check(SHARED + "shared v1.0.0|getrandom\n", SHARED), (0, ""))

    def test_a_new_extras_off_instance_of_a_shared_package_fails(self):
        status, diagnostic = self.check(SHARED, SHARED + "shared v1.0.0|\n")
        self.assertEqual(status, 1)
        self.assertIn("shared v1.0.0: default [std]; Extras-off [] / [std]", diagnostic)
