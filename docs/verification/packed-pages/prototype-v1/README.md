# Shared page-packing experiment, first version

Executed 2026-09-08 on macOS arm64. This is the explicitly requested native
page-packing prototype, not a production allocator or integrated runtime claim.
The production bridge still uses malloc/free at this revision.

`source/` retains the exact prototype and fixture used for the raw matrix.
`matrix/identity.json` binds the three compilation inputs and executable hash.
The linked Ghostty pin is `82232ecde55405559dec29c5466cb9e39938cb41`, built with
Zig 0.16.0, ReleaseFast, baseline CPU using `scripts/native/bootstrap.py`.
`run.py` preserves the exact commands and checks its input/binary identity at
completion. The shared build cache later replaced the executable when the
isolated CI fix ran its gate; the retained hash and source remain its identity.

All 120 native lifecycle cases passed: five repeats of 1/32 models, repetitive or
varied 10,000-line content, compression off/on, and allocator modes 1 (malloc),
3 (individual mappings from 4 KiB), and 4 (packed small allocations plus larger
individual mappings). Each records empty, filled, compressed, encoded, parked,
settled, READY, restored and cleanup states; requested/mapped bytes, mapping
counts, macOS RSS/charged footprint, process high-water RSS, disk bytes, and
encode/READY/history timing. Full formatted history and continuation comparisons
passed for every restored terminal. Files were removed and mode-4 mappings and
requested allocations returned to zero. These files are native experiment
snapshots, not the runtime's encrypted disk adapter.

For 32 varied compressed models, median additional charged footprint after
parking was 44,154,952 bytes with malloc, 573,440 with mode 3, and 65,536 with
mode 4. Mode 4 has higher empty-model and some restore costs. The measurement
uses one global, single-threaded prototype pool; per-owner pools can retain more
partially filled pages. These numbers cannot choose a production capacity policy
or establish an integrated latency regression baseline.

`allocator-sanitizers.jsonl` records the separate C allocator stress run compiled
with Clang `-O1 -g -Wall -Wextra -Werror -fsanitize=address,undefined`: 20 rounds,
2,048 concurrent slots, varying 0–8,999-byte allocations, payload/16-byte alignment
checks and fragmented releases. All 8,450 mappings were released. This instruments
the prototype itself; it does not instrument the linked Zig engine.

Independent review found production-adaptation blockers despite the passing
measured configuration: unmap performed inside assert is removed by NDEBUG,
64 KiB OS pages exceed the fixed bitmap, the pinned ABI passes log2 alignment,
and requested-byte limits alone cannot bound physical fragmentation. See
`../../../reviews/packed-pages-prototype.md`. Current tests used assertions
and 16 KiB macOS pages. No claim is made for the failing/unmeasured configurations.
Production adaptation, bounded failure tests, per-owner measurements, Linux
execution and integrated parking/resource qualification remain required.
