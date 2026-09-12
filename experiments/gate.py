"""Pure validation and comparison logic. All comparisons fail closed."""
from __future__ import annotations

import collections
import hashlib
import json
import math
import statistics
from pathlib import Path

SCHEMA = 2
RELATIVE_TOLERANCE = 0.10


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def number(value, name):
    require(type(value) in (int, float) and math.isfinite(value), f"invalid number: {name}")
    return value


def validate_memory(row):
    for key in ("rss_bytes", "virtual_bytes", "threads"):
        require(number(row[key], key) >= 0, f"negative memory metadata: {key}")
    source = row["memory_source"]
    require(source in ("proc-smaps-rollup", "proc-pid-rusage"), "unknown memory source")
    required = ("pss_bytes", "private_bytes") if source == "proc-smaps-rollup" else ("charged_footprint_bytes",)
    for key in required:
        require(number(row[key], key) >= 0, f"negative OS memory: {key}")
    unavailable = ("charged_footprint_bytes",) if source == "proc-smaps-rollup" else ("pss_bytes", "private_bytes")
    require(all(row[key] is None for key in unavailable), "incompatible OS memory counters")


RAMP_CHECKSUM = 256 * 255 // 2


def validate_handoff(case, row):
    """Placement movement is only evidence if ownership, byte order and the
    worker ceiling all held while descriptors moved between readers."""
    # Every workload argument, not just the two that shape the thread count: a
    # row measured at a different offered rate, window or duration answers a
    # different question, and comparing it against this case's numbers would be
    # comparing two experiments.
    args = case["args"]
    ptys, active, duration = int(args[2]), int(args[4]), int(args[5])
    rate, cycles, window, probe = (int(args[6]), int(args[7]),
                                   int(args[8]), int(args[9]))
    sessions, probes, workers = row["ptys"], row["probe_ptys"], row["reader_workers"]
    require(row["cycles"] == cycles and probes == 1, "handoff workload differs")
    require(sessions == ptys, "handoff ran a different PTY count")
    require(row["duration_ms"] == duration, "handoff ran for a different duration")
    require(row["offered_bytes_per_sec_per_producer"] == rate,
            "handoff ran at a different offered rate")
    require(row["window_ms"] == window and row["probe_ms"] == probe,
            "handoff measured over a different window")
    require(row["active_producers"] == active and len(row["producers"]) == active,
            "missing independent producers")
    require(row["dedicated"]["threads"] - row["base"]["threads"] == sessions + probes + workers,
            "dedicated placement did not run one reader per live PTY")
    require(row["peak_threads"] <= row["base"]["threads"] + sessions + probes + workers,
            "placement exceeded its bounded worker count")
    require(row["disordered_bytes"] == 0, "handoff lost, duplicated or reordered bytes")
    require(row["bytes"] == sum(p["bytes"] for p in row["producers"]), "producer totals differ")
    require(all(p["bytes"] > 0 for p in row["producers"]), "a producer made no progress")
    require(row["bytes"] % 256 == 0 and row["checksum"] == row["bytes"] // 256 * RAMP_CHECKSUM,
            "delivered bytes do not match the producer ramp")
    for direction in ("to_shared", "to_dedicated"):
        measured = sum(row[f"{direction}_{group}_us"]["samples"] for group in ("active", "quiet")
                       if row[f"{direction}_{group}_us"] is not None)
        require(measured == cycles * sessions, f"missing {direction} migrations")
    require(row["reader_threads_created"] == cycles * (sessions + probes),
            "reader threads created differ from the measured wake migrations")
    require(row["probe_roundtrips"] == row["dedicated_roundtrip"]["samples"] + row["shared_roundtrip"]["samples"],
            "probe round trips and delivered probe bytes disagree")
    require(row["dedicated_roundtrip"]["samples"] > 0 and row["shared_roundtrip"]["samples"] > 0,
            "a placement was never probed")
    if active:
        for placement in ("dedicated", "shared"):
            require(number(row[placement + "_mib_per_sec"], "throughput") > 0,
                    f"{placement} placement delivered nothing")


def validate_record(case, data):
    require(isinstance(data, list) and data, f"empty output: {case['id']}")
    kind = case["kind"]
    if kind.startswith("pty-"):
        require(len(data) == 1, "PTY fixture must emit exactly one record")
        row = data[0]
        require(row.get("verified") is True and row.get("protocol") == SCHEMA, "PTY verification failed")
        require(row.get("case") == kind[4:], "unexpected PTY case")
        require(row.get("model") == case["args"][1], "reader model mismatch")
        require(row.get("ptys") == int(case["args"][2]), "PTY count mismatch")
        require(row.get("read_buffer_bytes") == int(case["args"][3]), "buffer mismatch")
        stages = ("base", "resident", "cleaned") + (("dedicated",) if kind == "pty-handoff" else ())
        for stage in stages:
            validate_memory(row[stage])
        require(row["resident"]["threads"] - row["base"]["threads"] == row["reader_workers"],
                "actual worker count differs from configured readers")
        for field in ("descriptors", "threads"):
            require(row["base"][field] == row["cleaned"][field], f"unreleased {field}")
        if kind == "pty-handoff":
            validate_handoff(case, row)
        elif kind != "pty-idle":
            require(number(row["bytes"], "bytes") > 0, "empty transfer")
            require(number(row["aggregate_mib_per_sec"], "throughput") > 0, "invalid throughput")
        if kind == "pty-serial":
            require(row["bytes"] == int(case["args"][2]) * int(case["args"][4]) * 1048576,
                    "serial total does not match workload")
            require(row["sequential_handshake"]["samples"] == 2000, "missing handshake samples")
        if kind == "pty-concurrent":
            active = int(case["args"][4])
            require(row["active_producers"] == active and len(row["producers"]) == active,
                    "missing independent producers")
            require(row["bytes"] == sum(p["bytes"] for p in row["producers"]), "producer totals differ")
            require(all(p["bytes"] > 0 for p in row["producers"]), "a producer made no progress")
            require(row["under_load_roundtrip"]["samples"] > 0, "no under-load probes")
    elif kind == "native-lifecycle":
        expected = {"baseline", "empty", "filled", "after_compression", "encoded", "parked",
                    "parked_settled", "ready", "restored", "cleanup"}
        stages = [r["stage"] for r in data if r.get("kind") == "memory"]
        require(len(stages) == len(expected) and set(stages) == expected, "missing/duplicate native stages")
        for stage in (r for r in data if r.get("kind") == "memory"):
            validate_memory(stage)
        summaries = [r for r in data if r.get("kind") == "summary"]
        require(len(summaries) == 1, "missing/duplicate native summary")
        row = summaries[0]
        for field, argument in (("n", 0), ("lines", 1), ("varied", 2), ("compression", 3), ("cap_mib", 4)):
            require(row[field] == int(case["args"][argument]), f"native workload differs: {field}")
        require(row["verified_terminals"] == int(case["args"][0]), "native restore count differs")
        require(row["skipped_pages"] == 0, "native history was skipped in a quiescent fixture")
        for stage in (r for r in data if r.get("kind") == "memory"
                      and r.get("stage") in ("parked", "cleanup")):
            require(stage["tracked_native_bytes"] == 0 and stage["mapped_allocator_bytes"] == 0,
                    "native allocator retains live owned objects")
        # New fixtures emit separate pool counters alongside the OS/native
        # memory records. Older recorded runs contain memory records only.
        packed = [r for r in data if r.get("kind") == "packed_pool"]
        if packed:
            require(len(packed) == len(expected) and {r["stage"] for r in packed} == expected,
                    "missing/duplicate packed stages")
            for stage in (r for r in packed if r["stage"] in ("parked", "cleanup")):
                for key in ("requested", "mapped", "maps", "unmaps"):
                    require(number(stage[key], key) >= 0, f"negative packed counter: {key}")
                require(stage["requested"] == 0 and stage["mapped"] == 0
                        and stage["maps"] == stage["unmaps"],
                        "packed pool retains live owned objects or mappings")
    elif kind == "native-codec":
        require(len(data) == 30 and {r["sample"] for r in data} == set(range(30)),
                "missing/duplicate codec samples")
        require(all(r.get("kind") == "codec" for r in data), "unexpected codec records")
        require(all(r["lines"] == int(case["args"][0]) and r["varied"] == int(case["args"][1]) for r in data),
                "codec workload differs")
    elif kind == "native-correctness":
        expected = {"ground", "utf8", "csi", "osc", "dcs", "alternate"}
        require(len(data) == 6 and {r["case"] for r in data} == expected, "missing native correctness cases")
        flags = ("roundtrip", "binary_roundtrip_equal", "resume_and_resize", "replies",
                 "truncated_and_corrupt_rejected")
        require(all(r.get(f) is True for r in data for f in flags), "native correctness failure")
    else:
        raise ValueError(f"unknown case kind: {kind}")


def metrics(case, data):
    """Return (value, direction, absolute noise allowance). Memory stays OS-specific."""
    out = {}

    def add(key, value, direction="lower", floor=0.0):
        if value is not None:
            out[key] = {"value": number(value, key), "direction": direction, "absolute_tolerance": floor}

    kind = case["kind"]
    memory_keys = ("rss_bytes", "pss_bytes", "private_bytes", "charged_footprint_bytes", "live_heap_bytes")
    if kind.startswith("pty-"):
        r = data[0]
        if kind == "pty-idle":
            add("idle_cpu_percent", r["idle_cpu_percent"], floor=0.05)
        elif kind == "pty-handoff":
            # Transition cost, then the two steady placements the transition
            # moves between. `added_*` below is the shared placement; the
            # dedicated placement is reported under `dedicated_added_*`.
            for direction in ("to_shared", "to_dedicated"):
                for group in ("active", "quiet"):
                    window = r[f"{direction}_{group}_us"]
                    if window is not None:
                        add(f"{direction}_{group}_p50_us", window["p50_us"], floor=1.0)
                        add(f"{direction}_{group}_p99_us", window["p99_us"], floor=1.0)
            for placement in ("dedicated", "shared"):
                add(f"{placement}_roundtrip_p99_us", r[f"{placement}_roundtrip"]["p99_us"], floor=1.0)
                add(f"{placement}_cpu_percent", r[f"{placement}_cpu_percent"], floor=0.05)
                if int(case["args"][4]):
                    add(f"{placement}_mib_per_sec", r[f"{placement}_mib_per_sec"], "higher")
            for key in memory_keys:
                if r["dedicated"].get(key) is not None:
                    add("dedicated_added_" + key, max(0, r["dedicated"][key] - r["base"][key]),
                        floor=1024 if key == "live_heap_bytes" else 65536)
        else:
            add("aggregate_mib_per_sec", r["aggregate_mib_per_sec"], "higher")
            add("owner_cpu_ms_per_mib", r["owner_cpu_ms_per_mib"])
            latency = r["sequential_handshake" if kind == "pty-serial" else "under_load_roundtrip"]
            add("roundtrip_p99_us", latency["p99_us"], floor=1.0)
        for key in memory_keys:
            if r["resident"].get(key) is not None:
                add("added_" + key, max(0, r["resident"][key] - r["base"][key]),
                    floor=1024 if key == "live_heap_bytes" else 65536)
        add("reader_workers", r["reader_workers"])
    elif kind == "native-lifecycle":
        stages = {r["stage"]: r for r in data if r.get("kind") == "memory"}
        r = next(r for r in data if r.get("kind") == "summary")
        for key in ("feed_cpu_ms", "compression_cpu_ms", "encode_cpu_ms", "ready_p50_us", "ready_p99_us",
                    "history_sum_us", "snapshot_bytes"):
            add(key, r[key], floor=1.0 if key.endswith("_us") else 0.0)
        for stage in ("filled", "after_compression", "parked_settled", "restored"):
            for key in ("rss_bytes", "pss_bytes", "private_bytes", "charged_footprint_bytes"):
                if stages[stage].get(key) is not None:
                    add(stage + ".added_" + key, max(0, stages[stage][key] - stages["baseline"][key]), floor=65536)
    elif kind == "native-codec":
        for key in ("encode_us", "ready_us", "full_decode_us"):
            add(key, statistics.median(number(r[key], key) for r in data), floor=1.0)
    return out


def summarize(metadata, records):
    cases = {case["id"]: case for case in metadata["cases"]}
    repetitions = metadata["repetitions"]
    require(len(cases) == len(metadata["cases"]), "duplicate case definitions")
    seen = set()
    grouped = collections.defaultdict(list)
    for record in records:
        identity = (record.get("case_id"), record.get("iteration"))
        require(identity not in seen, f"duplicate result: {identity}")
        seen.add(identity)
        require(identity[0] in cases and type(identity[1]) is int and 0 <= identity[1] < repetitions,
                f"unexpected result: {identity}")
        require(record.get("status") == "passed", f"failed fixture: {identity}")
        case = cases[identity[0]]
        validate_record(case, record["data"])
        grouped[identity[0]].append(metrics(case, record["data"]))
    require(len(seen) == len(cases) * repetitions, "incomplete run: expected cases/repetitions are missing")
    result = {}
    for identity, runs in grouped.items():
        require(all(set(r) == set(runs[0]) for r in runs), f"inconsistent metric coverage: {identity}")
        result[identity] = {}
        for key in runs[0]:
            values = [r[key]["value"] for r in runs]
            result[identity][key] = {"median": statistics.median(values), "min": min(values), "max": max(values),
                                     "samples": len(values), "direction": runs[0][key]["direction"],
                                     "absolute_tolerance": runs[0][key]["absolute_tolerance"]}
    return {"schema": SCHEMA, "complete": True, "profile": metadata["profile"],
            "environment": metadata["comparison_environment"], "workload_sha256": digest(metadata["cases"]),
            "source_sha256": metadata["source_sha256"], "repetitions": repetitions,
            "minimum_baseline_repetitions": metadata["minimum_baseline_repetitions"], "cases": result}


def load_results(path):
    path = Path(path)
    metadata = json.loads((path / "metadata.json").read_text())
    records = [json.loads(line) for line in (path / "results.jsonl").read_text().splitlines() if line.strip()]
    return summarize(metadata, records)


def validate_baseline(value):
    require(value.get("schema") == SCHEMA and value.get("complete") is True, "invalid baseline schema/status")
    require(value["repetitions"] >= max(5, value["minimum_baseline_repetitions"]),
            "a performance baseline requires at least five complete repetitions")
    require(bool(value.get("cases")), "empty baseline")
    for identity, case in value["cases"].items():
        for key, metric in case.items():
            require(metric["samples"] == value["repetitions"], f"missing metric samples: {identity}/{key}")
            number(metric["median"], key)
            require(metric["direction"] in ("lower", "higher"), f"invalid metric direction: {key}")
            require(number(metric["absolute_tolerance"], key) >= 0, f"negative tolerance: {key}")


def compare(baseline, current):
    validate_baseline(baseline)
    validate_baseline(current)
    require(baseline["environment"] == current["environment"],
            "incompatible platform/machine/toolchain; use a baseline from the same environment")
    require(baseline["workload_sha256"] == current["workload_sha256"], "incompatible workload")
    require(set(baseline["cases"]) == set(current["cases"]), "case coverage differs")
    regressions = []
    checked = 0
    for identity, metrics_now in current["cases"].items():
        previous = baseline["cases"][identity]
        require(set(previous) == set(metrics_now), f"metric coverage differs: {identity}")
        for key, now in metrics_now.items():
            old = previous[key]
            require(old["direction"] == now["direction"] and old["absolute_tolerance"] == now["absolute_tolerance"],
                    f"metric contract changed: {identity}/{key}")
            a, b = number(old["median"], key), number(now["median"], key)
            allowance = max(abs(a) * RELATIVE_TOLERANCE, old["absolute_tolerance"])
            change = b - a if old["direction"] == "lower" else a - b
            checked += 1
            if change > allowance:
                regressions.append({"case_id": identity, "metric": key, "baseline": a, "current": b,
                                    "allowed_change": allowance, "direction": old["direction"]})
    return {"schema": SCHEMA, "passed": not regressions, "metrics_checked": checked,
            "relative_tolerance": RELATIVE_TOLERANCE, "regressions": regressions,
            "baseline_source_sha256": baseline["source_sha256"], "current_source_sha256": current["source_sha256"]}
