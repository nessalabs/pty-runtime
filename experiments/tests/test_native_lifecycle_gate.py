"""Actual mixed native records retain independent memory and pool cleanup checks."""
import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import gate

CASE = {"id": "native-packed", "kind": "native-lifecycle",
        "args": ["1", "10000", "0", "0", "4", "0", "4"]}


def records():
    path = Path(__file__).parent / "fixtures/native-lifecycle-packed.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()]


class NativeLifecycleGate(unittest.TestCase):
    def test_real_interleaved_pool_and_memory_records_validate_and_summarize(self):
        data = records()
        self.assertEqual(data[0]["kind"], "packed_pool")
        self.assertNotIn("tracked_native_bytes", data[0])
        metadata = dict(profile="schema-test", cases=[CASE], repetitions=1,
                        comparison_environment={}, source_sha256="recorded-fixture",
                        minimum_baseline_repetitions=5)
        summary = gate.summarize(metadata, [dict(case_id=CASE["id"], iteration=0,
                                               status="passed", data=data)])
        self.assertTrue(summary["complete"])
        self.assertGreater(summary["cases"][CASE["id"]]["snapshot_bytes"]["median"], 0)

    def test_memory_cleanup_retention_is_still_rejected(self):
        for stage in ("parked", "cleanup"):
            for field in ("tracked_native_bytes", "mapped_allocator_bytes"):
                with self.subTest(stage=stage, field=field):
                    data = records()
                    row = next(r for r in data if r["kind"] == "memory" and r["stage"] == stage)
                    row[field] = 1
                    with self.assertRaisesRegex(ValueError, "native allocator retains"):
                        gate.validate_record(CASE, data)

    def test_packed_cleanup_rejects_live_bytes_or_unreleased_mappings(self):
        for stage in ("parked", "cleanup"):
            for field in ("requested", "mapped", "maps"):
                with self.subTest(stage=stage, field=field):
                    data = records()
                    row = next(r for r in data if r["kind"] == "packed_pool" and r["stage"] == stage)
                    row[field] += 1
                    with self.assertRaisesRegex(ValueError, "packed pool retains"):
                        gate.validate_record(CASE, data)

    def test_legacy_memory_only_records_keep_identical_metrics(self):
        data = records()
        legacy = [copy.deepcopy(r) for r in data if r["kind"] != "packed_pool"]
        gate.validate_record(CASE, legacy)
        self.assertEqual(gate.metrics(CASE, data), gate.metrics(CASE, legacy))

    def test_partial_packed_stage_inventory_cannot_hide_cleanup(self):
        data = [r for r in records() if not (r["kind"] == "packed_pool" and r["stage"] == "cleanup")]
        with self.assertRaisesRegex(ValueError, "missing/duplicate packed stages"):
            gate.validate_record(CASE, data)


if __name__ == "__main__":
    unittest.main()
