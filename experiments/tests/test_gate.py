import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import gate
import run


def evidence(repetitions=5):
    case = {"id": "serial", "kind": "pty-serial", "binary": "pty",
            "args": ["serial", "shared", "1", "1024", "1"]}
    metadata = {"profile": "test", "cases": [case], "repetitions": repetitions,
                "comparison_environment": {"system": "Linux", "architecture": "x86_64"},
                "source_sha256": "fixture", "minimum_baseline_repetitions": 5}
    baseline = {"threads": 1, "descriptors": 3, "rss_bytes": 1000000, "pss_bytes": 900000,
                "private_bytes": 800000, "charged_footprint_bytes": None, "live_heap_bytes": 1000,
                "virtual_bytes": 2000000, "memory_source": "proc-smaps-rollup"}
    resident = dict(baseline, threads=2, descriptors=7, rss_bytes=1200000, pss_bytes=1100000,
                    private_bytes=1000000, live_heap_bytes=5000)
    row = {"protocol": 2, "case": "serial", "model": "shared", "ptys": 1, "read_buffer_bytes": 1024,
           "verified": True, "bytes": 1048576, "aggregate_mib_per_sec": 100.0, "owner_cpu_ms_per_mib": 2.0,
           "reader_workers": 1, "sequential_handshake": {"samples": 2000, "p99_us": 10.0},
           "base": baseline, "resident": resident, "cleaned": baseline}
    records = [{"case_id": "serial", "iteration": n, "status": "passed", "data": [copy.deepcopy(row)]}
               for n in range(repetitions)]
    return metadata, records


class GateTests(unittest.TestCase):
    def summary(self):
        return gate.summarize(*evidence())

    def test_identical_baseline_passes(self):
        result = gate.compare(self.summary(), self.summary())
        self.assertTrue(result["passed"])
        self.assertGreater(result["metrics_checked"], 0)

    def test_throughput_regression_fails(self):
        reference = self.summary()
        current = copy.deepcopy(reference)
        current["cases"]["serial"]["aggregate_mib_per_sec"]["median"] = 80
        result = gate.compare(reference, current)
        self.assertFalse(result["passed"])
        self.assertEqual(result["regressions"][0]["metric"], "aggregate_mib_per_sec")

    def test_memory_regression_fails(self):
        reference = self.summary()
        current = copy.deepcopy(reference)
        current["cases"]["serial"]["added_live_heap_bytes"]["median"] *= 2
        self.assertFalse(gate.compare(reference, current)["passed"])

    def test_small_noise_passes(self):
        reference = self.summary()
        current = copy.deepcopy(reference)
        current["cases"]["serial"]["roundtrip_p99_us"]["median"] = 10.5
        self.assertTrue(gate.compare(reference, current)["passed"])

    def test_different_platform_is_rejected(self):
        current = self.summary()
        current["environment"]["system"] = "Darwin"
        with self.assertRaisesRegex(ValueError, "incompatible platform"):
            gate.compare(self.summary(), current)

    def test_different_workload_is_rejected(self):
        current = self.summary()
        current["workload_sha256"] = "different"
        with self.assertRaisesRegex(ValueError, "incompatible workload"):
            gate.compare(self.summary(), current)

    def test_missing_repetition_is_rejected(self):
        meta, rows = evidence()
        with self.assertRaisesRegex(ValueError, "incomplete"):
            gate.summarize(meta, rows[:-1])

    def test_duplicate_result_is_rejected(self):
        meta, rows = evidence()
        with self.assertRaisesRegex(ValueError, "duplicate result"):
            gate.summarize(meta, rows + rows[:1])

    def test_failed_result_cannot_be_omitted(self):
        meta, rows = evidence()
        rows[-1]["status"] = "failed"
        with self.assertRaisesRegex(ValueError, "failed fixture"):
            gate.summarize(meta, rows)

    def test_nonfinite_measurement_is_rejected(self):
        meta, rows = evidence()
        rows[0]["data"][0]["aggregate_mib_per_sec"] = float("nan")
        with self.assertRaisesRegex(ValueError, "invalid number"):
            gate.summarize(meta, rows)

    def test_wrong_byte_total_is_rejected(self):
        meta, rows = evidence()
        rows[0]["data"][0]["bytes"] -= 1
        with self.assertRaisesRegex(ValueError, "serial total"):
            gate.summarize(meta, rows)

    def test_descriptor_leak_is_rejected(self):
        meta, rows = evidence()
        rows[0]["data"][0]["cleaned"] = dict(rows[0]["data"][0]["cleaned"], descriptors=4)
        with self.assertRaisesRegex(ValueError, "unreleased descriptors"):
            gate.summarize(meta, rows)

    def test_missing_latency_samples_are_rejected(self):
        meta, rows = evidence()
        rows[0]["data"][0]["sequential_handshake"]["samples"] = 1999
        with self.assertRaisesRegex(ValueError, "missing handshake"):
            gate.summarize(meta, rows)

    def test_wrong_os_memory_counter_is_rejected(self):
        meta, rows = evidence()
        rows[0]["data"][0]["resident"]["charged_footprint_bytes"] = 1000
        with self.assertRaisesRegex(ValueError, "incompatible OS memory"):
            gate.summarize(meta, rows)

    def test_extra_worker_is_rejected(self):
        meta, rows = evidence()
        rows[0]["data"][0]["resident"]["threads"] = 3
        with self.assertRaisesRegex(ValueError, "actual worker count"):
            gate.summarize(meta, rows)

    def test_smoke_run_cannot_become_a_performance_baseline(self):
        with self.assertRaisesRegex(ValueError, "five complete"):
            gate.validate_baseline(gate.summarize(*evidence(1)))

    def test_missing_metric_cannot_pass_comparison(self):
        current = self.summary()
        del current["cases"]["serial"]["roundtrip_p99_us"]
        with self.assertRaisesRegex(ValueError, "metric coverage"):
            gate.compare(self.summary(), current)

    def test_results_are_revalidated_from_raw_records(self):
        meta, rows = evidence()
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "metadata.json").write_text(json.dumps(meta))
            (root / "results.jsonl").write_text("\n".join(json.dumps(row) for row in rows))
            (root / "summary.json").write_text('{"complete":true}')
            self.assertEqual(gate.load_results(root), self.summary())

    def test_fixture_timeout_is_reported_and_reaped(self):
        with tempfile.TemporaryDirectory() as d:
            log = Path(d) / "timeout"
            with self.assertRaises(subprocess.TimeoutExpired):
                run.checked([sys.executable, "-c", "import time; time.sleep(30)"], timeout=0.1, log=log)
            self.assertIn("PROCESS GROUP TERMINATED", Path(str(log) + ".stderr").read_text())


if __name__ == "__main__":
    unittest.main()
