#!/usr/bin/env python3
"""Read the 25 retained trials only; never launch the measured program."""
import csv
import hashlib
import json
import statistics
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
INPUT = ROOT / "docs/verification/release"
manifest = {}


def read(path):
    data = path.read_bytes()
    manifest[str(path.relative_to(ROOT))] = hashlib.sha256(data).hexdigest()
    return data


def one(events, name, **where):
    rows = [e for e in events if e["event"] == name and all(e.get(k) == v for k, v in where.items())]
    assert len(rows) == 1, (name, where, len(rows))
    return rows[0]


def distribution(values):
    return {"min": min(values), "median": statistics.median(values), "mean": statistics.mean(values), "max": max(values)}


trials, producers, identities = [], [], []
groups = [("256-pre", "candidate5-depth-sweep", "capacity-projected", 256),
          ("128", "candidate5-depth-sweep", "capacity-depth-128", 128),
          ("64", "candidate5-depth-sweep", "capacity-depth-64", 64),
          ("32", "candidate5-depth-sweep", "capacity-depth-32", 32),
          ("256-post", "candidate5-depth-post-control", "capacity-projected", 256)]
for group, directory, case, depth in groups:
    summary = json.loads(read(INPUT / directory / "summary.json"))
    for trial in range(1, 6):
        path = INPUT / directory / f"{case}-{trial}.jsonl"
        events = [json.loads(line) for line in read(path).splitlines()]
        identity = one(events, "identity")
        identities.append({k: identity[k] for k in ["platform", "machine", "source_head", "diff_sha256", "binary_sha256", "source_inventory", "host"]})
        config = identity["configuration"]
        assert all(config[k] == v for k, v in dict(sessions=64, active=16, rate=0, chunk=4093, observers=1, cols=80, rows=24, raw=False, seconds=60, warmup=10, mode="saturation").items())
        assert one(events, "start")["projection_staging_slots_per_session"] == depth
        result = one(events, "trial_result")
        assert result["passed"] and result["full_duration_trial"]
        assert one(events, "complete")
        summary_row = [r for r in summary["results"] if r["case"] == case and r["trial"] == trial]
        assert len(summary_row) == 1 and summary_row[0]["passed"]
        assert summary_row[0]["latency_targets_passed"] == result["latency_targets_passed"]
        lat = {e["boundary"]: e for e in events if e["event"] == "latency"}
        targets = [e for e in events if e["event"] == "latency_target"]
        assert len(targets) == 5 and all(e["measurement_complete"] for e in targets)
        assert all(not e["failures"] and not e["unavailable"] for e in lat.values())
        throughput = one(events, "throughput")
        assert throughput["producer_mode"] == "unpaced" and throughput["offered_bytes_per_second"] is None
        total = 0
        active = []
        for producer in range(64):
            write = one(events, "producer_writes", phase=1, producer=producer)
            done = one(events, "producer_done", phase=1, producer=producer)
            end = one(events, "producer_end", phase=1, producer=producer)
            ledger = one(events, "ledger", producer=producer)
            assert done["phase_bytes"] == end["phase_bytes"]
            assert ledger["total_bytes"] == done["bytes_total"] == ledger["verified_bytes"] + ledger["gap_bytes"]
            assert ledger["gap_bytes"] == 0 and not write["cap_exhausted"]
            total += done["phase_bytes"]
            if producer >= 16:
                assert done["phase_bytes"] == 0
                continue
            gib = done["phase_bytes"] / 2**30
            row = dict(group=group, trial=trial, producer=producer,
                       phase_bytes=done["phase_bytes"], throughput_MB_s=done["producer_bytes_per_second"] / 1e6,
                       write_syscall_s=write["write_syscall_ns"] / 1e9,
                       max_write_ms=write["max_write_ns"] / 1e6,
                       eagain_count=write["eagain_count"], partial_count=write["partial_write_count"],
                       write_calls=done["write_calls"], blocked_s=done["write_blocked_ns"] / 1e9,
                       max_backpressure_wait_ms=done["max_backpressure_wait_ns"] / 1e6,
                       syscall_s_per_GiB=write["write_syscall_ns"] / 1e9 / gib,
                       eagain_per_GiB=write["eagain_count"] / gib,
                       partial_per_GiB=write["partial_write_count"] / gib)
            producers.append(row)
            active.append(row)
        assert total == throughput["accepted_bytes"]
        assert abs(total / throughput["producer_window_seconds"] - throughput["accepted_bytes_per_second"]) < 1e-6
        budgets = [e for e in events if e["event"] == "budget"]
        assert all(e["used"] <= e["limit"] for e in budgets)
        assert all(e["used"] == 0 for e in budgets if e["phase"] == "forgotten")
        physical = [e for e in events if e["event"] == "physical_resources"]
        assert not any(e["zombies"] for e in physical)
        cpu = one(events, "cpu_interval")
        checkpoint = one(events, "checkpoint", phase="measurement_end")
        aggregate = one(events, "aggregate")
        assert aggregate["live_readers"] == 64 and aggregate["reader_scratch_allocated_bytes"] == 262144
        rtt = one(events, "fixture_rtt")
        assert not rtt["failures"] and not rtt["unavailable"]
        row = dict(group=group, trial=trial, slots=depth, correctness=True,
                   latency_passed=result["latency_targets_passed"],
                   throughput_MB_s=throughput["accepted_bytes_per_second"] / 1e6,
                   projected_p99_ms=lat["ProjectedOutput"]["p99_us"] / 1000,
                   resize_p99_ms=lat["ResizeDispatch"]["p99_us"] / 1000,
                   input_p99_ms=lat["InputDispatch"]["p99_us"] / 1000,
                   rtt_p99_ms=rtt["p99_us"] / 1000, rtt_samples=rtt["successes"],
                   owner_cpu_core_pct=cpu["categories"]["owner"]["core_percent"],
                   fixture_cpu_core_pct=cpu["categories"]["workload_fixture"]["core_percent"],
                   owner_rss_peak_MiB=max(p["rss_bytes"] for e in physical for p in e["processes"] if p["category"] == "owner") / 2**20,
                   rust_requested_peak_MiB=checkpoint["rust_requested_peak"] / 2**20,
                   staging_peak_MiB=max(e["used"] for e in budgets if e["name"] == "staging_bytes") / 2**20,
                   scratch_bytes=aggregate["reader_scratch_allocated_bytes"],
                   syscall_s=sum(p["write_syscall_s"] for p in active),
                   max_write_ms=max(p["max_write_ms"] for p in active),
                   eagain_count=sum(p["eagain_count"] for p in active),
                   partial_count=sum(p["partial_count"] for p in active),
                   producer_blocked_s_median=statistics.median(p["blocked_s"] for p in active),
                   producer_max_backpressure_wait_ms=max(p["max_backpressure_wait_ms"] for p in active),
                   producer_throughput_min_MB_s=min(p["throughput_MB_s"] for p in active),
                   producer_throughput_max_MB_s=max(p["throughput_MB_s"] for p in active))
        gib = total / 2**30
        row.update(syscall_s_per_GiB=row["syscall_s"] / gib,
                   eagain_per_GiB=row["eagain_count"] / gib, partial_per_GiB=row["partial_count"] / gib,
                   owner_cpu_core_pct_per_MB_s=row["owner_cpu_core_pct"] / row["throughput_MB_s"])
        trials.append(row)

assert len(trials) == 25 and len(producers) == 400
assert all(i == identities[0] for i in identities), "source/binary/host identity mismatch"
numeric = [k for k, v in trials[0].items() if isinstance(v, (float, int)) and not isinstance(v, bool) and k not in ["trial", "slots"]]
rollups = {group: {key: distribution([r[key] for r in trials if r["group"] == group]) for key in numeric} for group, *_ in groups}
comparisons = {}
for base in ["256-pre", "256-post"]:
    comparisons[f"32_vs_{base}_median_pct"] = {k: (rollups["32"][k]["median"] / rollups[base][k]["median"] - 1) * 100 for k in numeric if rollups[base][k]["median"]}
comparisons["256_post_vs_pre_median_pct"] = {k: (rollups["256-post"][k]["median"] / rollups["256-pre"][k]["median"] - 1) * 100 for k in numeric if rollups["256-pre"][k]["median"]}
for name, rows in [("trials", trials), ("producers", producers)]:
    with (HERE / f"{name}.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
identity = {k: v for k, v in identities[0].items() if k != "source_inventory"}
identity["source_inventory_sha256"] = hashlib.sha256(json.dumps(identities[0]["source_inventory"], sort_keys=True).encode()).hexdigest()
(HERE / "analysis.json").write_text(json.dumps(dict(identity=identity, groups=rollups, comparisons=comparisons, trials=trials, validation="25 complete trials; 400 active phase-1 producer rows; identities, byte ledgers, throughput, budgets, completion and summary rollups checked"), indent=2) + "\n")
(HERE / "input-sha256.txt").write_text("".join(f"{sha}  {path}\n" for path, sha in sorted(manifest.items())))
print(json.dumps({"groups": {g: {k: round(v["median"], 3) for k, v in values.items()} for g, values in rollups.items()}, "comparisons": comparisons}, indent=2))
