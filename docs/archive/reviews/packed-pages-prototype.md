# Packed-page allocator prototype review

The experiment supports further work on explicit page release, but this header is
not ready to become the production allocator. The global, single-threaded design
is appropriate to the supplied experiment. A per-owner version needs separate
physical mapping limits, lifetime ownership and new fragmentation measurements.
No production pool change is reviewed or approved by this report.

Reviewed `experiments/native/packed-pages.h`, `packed-pages-test.c`, mode 4 in
`native-memory.c`, the pinned Ghostty allocator bridge and the supplied raw matrix.
Review base: `52ad04c3519616e6cedea9ac8707406970a40ed7` plus the uncommitted prototype.
The matrix metadata contains 120 cases, all exit zero. The retained sanitizer result
reports 20 rounds, 2048 slots, 8450 mappings and 21,954,560 peak mapped bytes.
Those observations establish the exercised macOS workload, not the boundaries below.

## Concrete findings

- **P2 — Release depends on assertions being enabled.** `packed-pages.h:56` calls
  `munmap` inside `assert`. With `NDEBUG`, it never unmaps but decrements `mapped`
  and increments `unmaps`. An independent `clang -DNDEBUG` reproducer allocated
  100 bytes, freed it, then called `mincore` on its mapping: accounted mapped bytes
  were zero while `mincore` returned zero, proving the mapping remained present.
  The production operation must execute outside assertions, with a defined
  failure/retained-charge policy. The prototype test's success assertions also
  disappear under `NDEBUG`; a release-mode test needs checks that execute.

- **P2 — The bitmap assumes an OS page no larger than 16 KiB.** At lines 72–77,
  rounding the 16 KiB slab to a 64 KiB OS page gives 2044 slots of 32 bytes, but
  `occupied[8]` holds only 512 bits. Assertions reject the first allocation;
  assertion-disabled builds can index outside the bitmap after sufficient
  allocations. Linux arm64 qualification must include its supported page sizes.
  Derive bitmap capacity from slab policy, cap slot capacity independently from
  mapped length, or explicitly reject unsupported page sizes before mapping.

- **P2 — Alignment units need an explicit adapter contract.** The pinned C header
  describes byte alignment 1–16, but `src/lib/allocator.zig:99` forwards
  `@intFromEnum(std.mem.Alignment)`: zero means one byte, four means 16 bytes,
  and five means 32 bytes. Mode 4 returns only 16-byte aligned payloads while
  `native-memory.c:24` accepts values through 16. This admits higher alignment
  requests than it can fulfill if such a native allocation occurs. The current
  workload does not demonstrate such a request, so this is a contract boundary,
  not evidence of corruption in the 120 runs. Production should decode the pinned
  ABI, check the shift, and either satisfy the alignment or return allocation
  failure. Tests must exercise actual callback alignment values, including zero
  and a request above 16-byte alignment, rather than checking only `% 16 == 0`.

- **P2 before production — Requested bytes do not bound mapped footprint.** The
  experiment has no requested/mapped/object ceilings and increments counters
  without checked addition (`packed-pages.h:50,89`). Near-`SIZE_MAX` page rounding
  aborts through an assertion instead of a defined allocation rejection.
  Zero-byte objects consume slots without increasing `requested`. More generally,
  one tiny surviving object can retain an entire slab. A requested-byte ceiling
  alone therefore cannot bound mappings under adversarial fragmentation. Check
  all header/rounding/counter arithmetic before allocation and impose a separate
  physical mapping/slab or object bound. Allocation failure must leave existing
  objects and every charge unchanged.

These are open adaptation blockers, not claims that the assertion-enabled macOS
experiment violated its stated inputs. No prototype source was edited.

## Lifetime and organization

The existing available-list transitions are coherent for valid single-threaded
alloc/free pairs: full slabs leave the available list; the first free reinserts
one; the last free removes and releases it. Payload starts and slot strides give
16-byte alignment on the tested 64-bit ABI. Fresh anonymous mappings supply zeroed
bitmap/count/link fields. The random test exercises payload non-overlap and many
full/nonfull transitions. Free's supplied length must exactly match the allocation;
the header does not retain it. No resize/remap succeeds, so their refusal currently
preserves that invariant. Returning false/null for resize/remap is a legitimate
allocator behavior, though its copy cost belongs in performance measurements.

A production context should be a stable, explicitly owned pool shared only by the
native objects that actually share lifetime. A READY decoder and its returned
terminal may outlive one another; destroying the decoder must not destroy a pool
still referenced by the live model. Source buffers, exported copies and default
native page allocations must be inventoried separately. Verify the complete
allocation graph; `tracked_native_bytes` is only allocations that reach this
vtable, not a substitute for all native or physical memory.

Avoid a process-global pool or lock. Scheduler serialization can protect a
per-native-owner pool, but that must include allocation/free from feed, resize,
restoration, cancellation and teardown, including every callback entry. If
cross-thread release is allowed, define its synchronization explicitly. Keep pool
ownership outside movable callback contexts. Existing allocation handles must
remain valid on quota rejection and partial decoder failure.

Full slabs are not reachable from `available`; this structure alone cannot support
a final forced release or audit of all live mappings. Decide whether teardown
requires proven zero outstanding allocations, a separate all-slabs ledger, or an
owner arena teardown contract. Do not silently unmap memory still referenced by a
model. Record failed unmap as unreclaimed physical storage rather than reporting
zero. Preserve payload wiping; padding/headers and wrong-size frees need their own
contract if the pool promises broader erasure.

## Tests and measurements required next

1. Exercise every size-class edge, zero, 4080/4081-byte payloads on this ABI,
   page-rounding boundaries, near-`SIZE_MAX`, all supported alignments and OS page
   sizes. Inject mapping failure and release failure; verify unchanged existing
   allocations, truthful accounting and valid list state. Run with assertions
   enabled and disabled and ASan/UBSan.
2. Force each full-to-partial-to-empty transition, allocate after reinsertion,
   release in ascending/reverse/random order, and retain exactly one object per
   slab across repeated churn. Measure the resulting mapped/requested ratio and
   prove the hard mapped ceiling. The current random distribution often packs
   small classes densely and all objects are freed together at its last round.
3. Test multiple independent owners: park one while others remain active, destroy
   decoder before/after its returned model, fail restoration partway, and repeat
   live input/resize/park/restore/close under the actual scheduler. Verify owner
   isolation and zero mapping charges after final release. Sanitizers on a global
   allocator do not establish this per-owner lifetime contract.
4. Repeat 1/32/128 active and mixed hot/parked populations with per-owner pools.
   A global pool shares partially used slabs across all models; one owner with
   one live object in each of eight classes can instead retain eight 16 KiB slabs
   (128 KiB), before large mappings. That fixed slack multiplies across owners and
   grows on larger OS pages. Measure empty, filled, compressed, parked, READY,
   restored and closed states, with long-lived fragmented owners still present.
5. Retain requested bytes, mapped bytes, slab occupancy/slack, map/unmap counts,
   allocation failures and copy volume alongside RSS, macOS charged footprint,
   Linux PSS/private bytes, CPU and latency distributions. Measure churn and
   independent owners, not only a synchronized all-model-free point. The supplied
   roughly 44 MiB versus 64 KiB parked-footprint contrast is promising evidence for
   that workload; it does not predict marginal per-owner cost or release latency.

The bounded per-owner approach is worth prototyping after these contracts are
chosen. Keep the current evidence explicitly experimental until the real native
owner lifecycle, physical ceiling, platform matrix and failure paths are measured.
