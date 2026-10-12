"""Inventory must retain generated code owners and reject unbound sources."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "guardian_inventory", Path(__file__).resolve().parents[1] / "guardian/coverage_checks.py")
checks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checks)


class GeneratedOwners(unittest.TestCase):
    def setUp(self):
        self.helper = str(Path("helper.rs").resolve())
        self.installed = str(Path("pinned-thread-local.rs").resolve())
        self.export = {"type": "llvm.coverage.json.export", "data": [{
            "files": [{"filename": self.helper}, {"filename": self.installed}],
            "functions": [
                {"name": "helper", "filenames": [self.helper],
                 "regions": [[1, 1, 2, 1, 1, 0, 0, 0]]},
                {"name": "generated", "filenames": [self.installed],
                 "regions": [[71, 25, 71, 26, 0, 0, 0, 0]]}]}]}

    def test_bound_primary_and_its_zero_count_are_retained_verbatim(self):
        original = copy.deepcopy(self.export)
        data, owners = checks.inventory(self.export, {self.helper}, {self.installed})
        self.assertEqual(self.export, original)
        self.assertEqual(data, original["data"][0])
        self.assertTrue(owners[self.installed][0]["primary_owner"])
        self.assertEqual(owners[self.installed][0]["raw_region"], [71, 25, 71, 26, 0, 0, 0, 0])

    def test_an_unbound_primary_is_rejected(self):
        with self.assertRaises(checks.AuditError):
            checks.inventory(self.export, {self.helper}, set())

    def test_generated_code_cannot_replace_a_missing_compiled_owner(self):
        self.export["data"][0]["functions"].pop(0)
        with self.assertRaises(checks.AuditError):
            checks.inventory(self.export, {self.helper}, {self.installed})

    def test_metadata_only_primary_is_rejected(self):
        self.export["data"][0]["functions"][1]["regions"][0][7] = 1
        with self.assertRaises(checks.AuditError):
            checks.inventory(self.export, {self.helper}, {self.installed})

    def test_duplicate_file_summaries_remain_rejected(self):
        self.export["data"][0]["files"].append({"filename": self.installed})
        with self.assertRaises(checks.AuditError):
            checks.inventory(self.export, {self.helper}, {self.installed})
