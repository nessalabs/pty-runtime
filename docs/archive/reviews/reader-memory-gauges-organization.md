# Reader memory gauges: independent organization review

No P1/P2 organization, DDD/layering, file-size or public-ownership findings in the reviewed change. Exact SHA-256 identities and nonblank counts for eight files are recorded in `docs/verification/release/reader-memory-gauges-organization/source.json`; the largest is 183 nonblank lines.

Application diagnostics owns fixed atomic observations and the small `ReaderAllocation` lifetime token. It depends only on the standard library and introduces no process implementation, OS types, native pointers, allocator hooks or domain-policy changes. The token retains the shared diagnostics object and observed byte count, not the underlying buffer; its public contract explicitly requires creation after actual allocation, fixed capacity, and retention until buffer release. This is an adapter observation boundary, consistent with existing diagnostics recording, rather than an application-owned resource allocator.

Infrastructure owns `ReaderScratch`, the actual `Vec<u8>`, and the optional token. It measures the vector's actual capacity after successful allocation. Field declaration order releases the buffer before decrementing its gauges. The normal reader scope and caught callback-error path retain both through drain notification and then release them. There is no added unsafe code, per-byte instrumentation, new admission failure, quota change or extra heap allocation for the token. Diagnostics-disabled readers retain the same payload allocation behavior.

Counter reset preserves current reader gauges while clearing cumulative measurements, matching the existing distinction between live ownership and interval counters. Independent relaxed counters are not an atomic pair; the public guard documentation correctly calls concurrent snapshots approximate and exact totals require quiescence. This instrumentation enables measured scratch evidence but does not itself make every arbitrary fixture checkpoint quiescent or close the earlier ≤4 KiB proof gap.

Fixture reporting owns JSON fields and carries the diagnostics reference at runtime/ready/measurement/closed checkpoints. The pre-diagnostics baseline explicitly reports null. Formatting the two numeric gauge strings occurs before the requested-heap snapshot, so these small fixture allocations are included in that snapshot; they must remain conservatively charged or separately measured in a later memory decomposition, never mistaken for runtime control allocations.

Source inspection covers the two application lifetime/reset/unwind tests and two real-reader integration tests, including nondefault buffer sizes, independent exits with retained handles, and caught output panic followed by joined shutdown. This reviewer did not rerun builds/tests or start workloads; the author and coordinating agent own focused executions and the full gate. This clean organization review does not claim native-crash resolution, performance acceptance, or lifecycle/soak completion.

## Final Rustdoc-only identity addendum

Independently verified that removing the newly added inline-code backticks around the two Vec type/method references reproduces the previously reviewed hashes exactly. This fixes Rustdoc HTML interpretation only; the clean organization result remains valid. No implementation or tests changed in these files. Final SHA-256 values:

- `crates/application/src/diagnostics/counters.rs`: `0175fe984d6f9b91c026e0ef6a1fa3e478f39175c1b1dfd4e51715bb3de15beb`
- `crates/application/src/diagnostics/reader.rs`: `8c64b1997db65c04f837e8bd6574c2147a92491074538aea01b58c6e189c9933`
