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


def handoff_evidence(repetitions=5):
    """One migration cycle over four sessions plus the echo probe, on two shared
    workers. The fixture is a placement measurement only; nothing here describes
    runtime behaviour."""
    case = {"id": "handoff", "kind": "pty-handoff", "binary": "pty",
            "args": ["handoff", "shared-2", "4", "1024", "2", "6000", "262144", "1", "200", "200"]}
    metadata = {"profile": "test", "cases": [case], "repetitions": repetitions,
                "comparison_environment": {"system": "Linux", "architecture": "x86_64"},
                "source_sha256": "fixture", "minimum_baseline_repetitions": 5}
    memory = {"threads": 1, "descriptors": 3, "rss_bytes": 1000000, "pss_bytes": 900000,
              "private_bytes": 800000, "charged_footprint_bytes": None, "live_heap_bytes": 1000,
              "virtual_bytes": 2000000, "memory_source": "proc-smaps-rollup"}
    window = {"samples": 2, "p50_us": 40.0, "p95_us": 80.0, "p99_us": 90.0, "max_us": 90.0}
    roundtrip = dict(window, samples=10)
    row = {"protocol": 2, "case": "handoff", "model": "shared-2", "ptys": 4, "read_buffer_bytes": 1024,
           "verified": True, "cycles": 1, "probe_ptys": 1, "active_producers": 2, "reader_workers": 2,
           "duration_ms": 6000, "offered_bytes_per_sec_per_producer": 262144,
           "window_ms": 200, "probe_ms": 200,
           "base": memory, "cleaned": memory,
           "dedicated": dict(memory, threads=8, descriptors=15, rss_bytes=1600000),
           "resident": dict(memory, threads=3, descriptors=15, rss_bytes=1100000),
           "peak_threads": 8, "bytes": 8192, "checksum": 8192 // 256 * 32640, "disordered_bytes": 0,
           "producers": [{"bytes": 4096}, {"bytes": 4096}],
           "dedicated_mib_per_sec": 2.0, "shared_mib_per_sec": 1.9,
           "dedicated_cpu_percent": 1.0, "shared_cpu_percent": 1.2,
           "dedicated_roundtrip": roundtrip, "shared_roundtrip": roundtrip,
           "to_shared_active_us": window, "to_shared_quiet_us": window,
           "to_dedicated_active_us": window, "to_dedicated_quiet_us": window,
           "reader_threads_created": 5, "probe_roundtrips": 20}
    records = [{"case_id": "handoff", "iteration": n, "status": "passed", "data": [copy.deepcopy(row)]}
               for n in range(repetitions)]
    return metadata, records


class HandoffGateTests(unittest.TestCase):
    def summary(self):
        return gate.summarize(*handoff_evidence())

    def test_complete_handoff_evidence_validates(self):
        summary = self.summary()
        self.assertTrue(gate.compare(summary, self.summary())["passed"])
        self.assertIn("to_shared_quiet_p99_us", summary["cases"]["handoff"])
        self.assertIn("dedicated_added_rss_bytes", summary["cases"]["handoff"])

    def test_reordered_or_lost_bytes_are_rejected(self):
        meta, rows = handoff_evidence()
        rows[0]["data"][0]["disordered_bytes"] = 1
        with self.assertRaisesRegex(ValueError, "lost, duplicated or reordered"):
            gate.summarize(meta, rows)

    def test_a_row_from_another_workload_is_not_this_case_s_evidence(self):
        """Only the arguments shaping the thread count used to be compared.

        A stale or mislabelled row measured at a different offered rate,
        duration, window or PTY count therefore passed validation and entered
        the comparison as if it answered this case's question. Each of those
        inputs determines the throughput and latency being reported.
        """
        for field, value in (("duration_ms", 5000),
                             ("offered_bytes_per_sec_per_producer", 131072),
                             ("window_ms", 300),
                             ("probe_ms", 300),
                             ("ptys", 8)):
            with self.subTest(field=field):
                meta, rows = handoff_evidence()
                rows[0]["data"][0][field] = value
                with self.assertRaises(ValueError):
                    gate.summarize(meta, rows)

    def test_delivered_bytes_must_match_the_producer_ramp(self):
        meta, rows = handoff_evidence()
        rows[0]["data"][0]["checksum"] += 32640
        with self.assertRaisesRegex(ValueError, "producer ramp"):
            gate.summarize(meta, rows)

    def test_unbounded_worker_growth_is_rejected(self):
        meta, rows = handoff_evidence()
        rows[0]["data"][0]["peak_threads"] = 9
        with self.assertRaisesRegex(ValueError, "bounded worker count"):
            gate.summarize(meta, rows)

    def test_dedicated_placement_must_run_one_reader_per_pty(self):
        meta, rows = handoff_evidence()
        rows[0]["data"][0]["dedicated"] = dict(rows[0]["data"][0]["dedicated"], threads=7)
        with self.assertRaisesRegex(ValueError, "one reader per live PTY"):
            gate.summarize(meta, rows)

    def test_missing_migrations_are_rejected(self):
        meta, rows = handoff_evidence()
        rows[0]["data"][0]["to_dedicated_quiet_us"] = dict(
            rows[0]["data"][0]["to_dedicated_quiet_us"], samples=1)
        with self.assertRaisesRegex(ValueError, "missing to_dedicated migrations"):
            gate.summarize(meta, rows)

    def test_unprobed_placement_is_rejected(self):
        meta, rows = handoff_evidence()
        row = rows[0]["data"][0]
        row["shared_roundtrip"] = dict(row["shared_roundtrip"], samples=0)
        row["probe_roundtrips"] = 10
        with self.assertRaisesRegex(ValueError, "never probed"):
            gate.summarize(meta, rows)

    def test_slower_migration_is_a_regression(self):
        reference = self.summary()
        current = copy.deepcopy(reference)
        current["cases"]["handoff"]["to_shared_quiet_p99_us"]["median"] = 200.0
        result = gate.compare(reference, current)
        self.assertFalse(result["passed"])
        self.assertEqual(result["regressions"][0]["metric"], "to_shared_quiet_p99_us")

    def test_worse_shared_roundtrip_is_a_regression(self):
        reference = self.summary()
        current = copy.deepcopy(reference)
        current["cases"]["handoff"]["shared_roundtrip_p99_us"]["median"] *= 3
        self.assertFalse(gate.compare(reference, current)["passed"])

    def test_dedicated_placement_memory_growth_is_a_regression(self):
        reference = self.summary()
        current = copy.deepcopy(reference)
        current["cases"]["handoff"]["dedicated_added_rss_bytes"]["median"] *= 2
        self.assertFalse(gate.compare(reference, current)["passed"])

    def test_idle_handoff_case_reports_no_throughput_metric(self):
        meta, rows = handoff_evidence()
        meta["cases"][0]["args"][4] = "0"
        for record in rows:
            row = record["data"][0]
            row.update(active_producers=0, producers=[], bytes=0, checksum=0,
                       dedicated_mib_per_sec=0.0, shared_mib_per_sec=0.0)
            for direction in ("to_shared", "to_dedicated"):
                row[direction + "_active_us"] = None
                row[direction + "_quiet_us"] = dict(row[direction + "_quiet_us"], samples=4)
        summary = gate.summarize(meta, rows)
        self.assertNotIn("shared_mib_per_sec", summary["cases"]["handoff"])
        self.assertIn("to_shared_quiet_p99_us", summary["cases"]["handoff"])


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
