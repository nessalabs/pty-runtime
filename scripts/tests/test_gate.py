"""Adversarial checks against the real repository architecture gate."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("runtime_gate", ROOT / "scripts/gate.py")
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


class ArchitectureGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.baseline = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            cwd=ROOT))

    def setUp(self):
        self.metadata = copy.deepcopy(self.baseline)

    def package(self, name):
        return next(p for p in self.metadata["packages"] if p["name"] == name)

    def check(self):
        with patch.object(GATE.subprocess, "check_output",
                          return_value=json.dumps(self.metadata)):
            GATE.architecture()

    def dependency(self, owner="pty-runtime-application"):
        return self.package(owner)["dependencies"][0]

    def test_actual_workspace_is_accepted(self):
        self.check()

    def test_missing_each_required_package_is_rejected(self):
        for name in ("pty-runtime", "pty-runtime-domain", "pty-runtime-application",
                     "pty-runtime-infrastructure"):
            with self.subTest(name=name):
                self.metadata = copy.deepcopy(self.baseline)
                self.metadata["packages"].remove(self.package(name))
                with self.assertRaisesRegex(SystemExit, "Missing or relocated"):
                    self.check()

    def test_renamed_core_package_is_rejected(self):
        self.package("pty-runtime-domain")["name"] = "renamed-domain"
        with self.assertRaisesRegex(SystemExit, "Missing or relocated"):
            self.check()

    def test_relocated_package_is_rejected(self):
        self.package("pty-runtime-domain")["manifest_path"] = str(ROOT / "other/Cargo.toml")
        with self.assertRaisesRegex(SystemExit, "Missing or relocated"):
            self.check()

    def test_required_nonmember_is_rejected(self):
        package_id = self.package("pty-runtime-domain")["id"]
        self.metadata["workspace_members"].remove(package_id)
        with self.assertRaisesRegex(SystemExit, "not a workspace member"):
            self.check()

    def test_external_core_dependencies_in_all_declaration_kinds_are_rejected(self):
        for owner in ("pty-runtime-domain", "pty-runtime-application"):
            for kind in (None, "dev", "build"):
                for target in (None, 'cfg(target_os = "linux")'):
                    with self.subTest(owner=owner, kind=kind, target=target):
                        self.metadata = copy.deepcopy(self.baseline)
                        dependency = copy.deepcopy(self.dependency())
                        dependency.update(name="libc", path=None, kind=kind,
                                          target=target, optional=True, rename="os_api")
                        self.package(owner)["dependencies"].append(dependency)
                        with self.assertRaisesRegex(SystemExit, "Forbidden dependency"):
                            self.check()

    def test_correctly_named_dependency_pointing_elsewhere_is_rejected(self):
        self.dependency()["path"] = str(ROOT / "other-domain")
        with self.assertRaisesRegex(SystemExit, "points elsewhere"):
            self.check()

    def test_registry_dependency_impersonating_domain_is_rejected(self):
        self.dependency().pop("path", None)
        with self.assertRaisesRegex(SystemExit, "points elsewhere"):
            self.check()

    def test_renamed_alias_does_not_hide_wrong_path(self):
        self.dependency().update(rename="core", path=str(ROOT / "other-domain"))
        with self.assertRaisesRegex(SystemExit, "points elsewhere"):
            self.check()

    def test_valid_domain_alias_remains_allowed(self):
        self.dependency()["rename"] = "core"
        self.check()

    def test_infrastructure_to_facade_reverse_edge_is_rejected(self):
        dependency = copy.deepcopy(self.dependency())
        dependency.update(name="pty-runtime", path=str(ROOT))
        self.package("pty-runtime-infrastructure")["dependencies"].append(dependency)
        with self.assertRaisesRegex(SystemExit, "Reversed workspace dependency"):
            self.check()


if __name__ == "__main__":
    unittest.main()
