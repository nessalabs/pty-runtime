"""Validate complete source inventories and physical coverage counters."""
from collections import defaultdict
from pathlib import Path
import re

SATURATION = 2 ** 63 - 1


class AuditError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise AuditError(message)


def same_members(actual, expected, label):
    actual, expected = set(actual), set(expected)
    require(actual == expected,
            f"{label}: missing={sorted(expected - actual)}, extra={sorted(actual - expected)}")


def inventory(export, compiled, bound_source_files):
    """Retain compiled sources and bound imported code from selected objects.

    A standard-library macro can generate a function whose primary source is
    its definition, rather than a secondary alias of a helper function. Inlined
    dependency functions likewise retain their verified dependency source.
    """
    require(export.get("type") == "llvm.coverage.json.export", "not an LLVM coverage export")
    require(len(export["data"]) == 1, "ambiguous export object population")
    data = export["data"][0]
    compiled = set(map(str, compiled))
    measured = [str(Path(f["filename"]).resolve()) for f in data["files"]]
    require(len(measured) == len(set(measured)), "duplicate normalized file summaries")
    aliases = defaultdict(list)
    roots = set()
    for index, function in enumerate(data["functions"]):
        filenames = [str(Path(f).resolve()) for f in function["filenames"]]
        require(filenames and (filenames[0] in compiled or
                               filenames[0] in bound_source_files),
                f"function {function['name']} has an unselected source owner")
        if filenames[0] not in compiled:
            require(any(len(r) == 8 and r[5] == 0 and r[7] == 0
                        and (r[0], r[1]) < (r[2], r[3]) for r in function["regions"]),
                    f"bound imported primary owner has no code region: {function['name']}")
        roots.add(filenames[0])
        for position, region in enumerate(function["regions"]):
            require(len(region) == 8 and 0 <= region[5] < len(filenames),
                    f"invalid region file index in {function['name']}")
            source = filenames[region[5]]
            if source in compiled:
                continue
            require(source in bound_source_files, f"unowned source alias: {source}")
            require(region[7] == 0 and (region[0], region[1]) < (region[2], region[3]),
                    f"source alias is not a nonempty code region: {source}")
            aliases[source].append({"function_index": index, "function": function["name"],
                                    "primary_source": filenames[0], "region_index": position,
                                    "primary_owner": region[5] == 0,
                                    "file_id": region[5], "raw_region": region})
    same_members(roots & compiled, compiled, "compiled function source owners")
    same_members(measured, compiled | aliases.keys(), "complete exported source inventory")
    return data, dict(aliases)


def export_saturation(data):
    """Read original counters, including aliases with zero file-summary denominators."""
    saturated = []
    counter_count = 0
    maximum = 0

    def check(value, location):
        nonlocal counter_count, maximum
        require(isinstance(value, int) and not isinstance(value, bool) and value >= 0,
                f"invalid counter at {location}")
        counter_count += 1
        maximum = max(maximum, value)
        if value >= SATURATION:
            saturated.append({"location": location, "count": value})

    for i, function in enumerate(data["functions"]):
        check(function["count"], ["functions", i, "count"])
        for j, region in enumerate(function["regions"]):
            check(region[4], ["functions", i, "regions", j])
        for j, branch in enumerate(function["branches"]):
            for side in (4, 5):
                check(branch[side], ["functions", i, "branches", j, side])
        require(not function.get("mcdc_records"), "MC/DC records require an explicit audit parser")
    for i, source in enumerate(data["files"]):
        for j, segment in enumerate(source["segments"]):
            if segment[3]:
                check(segment[2], ["files", i, "segments", j])
        for j, branch in enumerate(source["branches"]):
            for side in (4, 5):
                check(branch[side], ["files", i, "branches", j, side])
        require(not source.get("mcdc_records"), "MC/DC file records require an explicit audit parser")
    return {"checked_count_values": counter_count, "maximum": maximum,
            "threshold": SATURATION, "saturated": saturated}


def parse_profile(text):
    """Parse the pinned llvm-profdata instrumentation text, never function hashes as counts."""
    lines = [line for line in text.splitlines() if line]
    records, i = {}, 0
    while i < len(lines):
        require(i + 6 <= len(lines), "truncated profile record")
        name = lines[i]
        require(not name.startswith("#"), "unsupported profile header")
        require(lines[i + 1] == "# Func Hash:" and lines[i + 3] == "# Num Counters:"
                and lines[i + 5] == "# Counter Values:", "unsupported profile record layout")
        require(re.fullmatch(r"[0-9]+", lines[i + 2]) is not None, "invalid function hash")
        require(re.fullmatch(r"[0-9]+", lines[i + 4]) is not None, "invalid counter count")
        size = int(lines[i + 4])
        values = lines[i + 6:i + 6 + size]
        require(size > 0 and len(values) == size, "truncated counter array")
        require(all(re.fullmatch(r"[0-9]+", v) for v in values), "invalid physical counter")
        key = (name, int(lines[i + 2]))
        require(key not in records, "duplicate physical function record")
        records[key] = list(map(int, values))
        i += 6 + size
    require(records, "empty profile dump")
    return records


def profile_conservation(raw_profiles, merged):
    totals = {}
    saturated = []
    for label, records in raw_profiles.items():
        for key, values in records.items():
            if key not in totals:
                totals[key] = [0] * len(values)
            require(len(totals[key]) == len(values), "counter array changed for one function identity")
            for i, value in enumerate(values):
                totals[key][i] += value
                if value >= SATURATION:
                    saturated.append({"profile": label, "function": key[0],
                                      "function_hash": key[1], "counter": i, "count": value})
    # The recorded merge uses -sparse: all-zero records may be omitted, not nonzero ones.
    nonzero = {key for key, values in totals.items() if any(values)}
    require(nonzero <= merged.keys(), "sparse merge lost a nonzero function record")
    require(merged.keys() <= totals.keys(), "merged profile added a function absent from raw profiles")
    for key, values in merged.items():
        require(values == totals[key], f"merged counters differ from raw sum for {key}")
        for i, value in enumerate(values):
            if value >= SATURATION:
                saturated.append({"profile": "merged", "function": key[0],
                                  "function_hash": key[1], "counter": i, "count": value})
    return {"raw_function_identities": len(totals), "merged_function_identities": len(merged),
            "zero_only_omitted": len(totals.keys() - merged.keys()),
            "physical_counter_count": sum(map(len, totals.values())),
            "maximum_raw_sum": max(value for values in totals.values() for value in values),
            "saturated": saturated, "sparse_merge_conserves_counters": True}
