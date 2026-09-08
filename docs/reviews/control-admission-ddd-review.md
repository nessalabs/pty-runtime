# Independent DDD/dependency review: control admission and bundled image validation

Scope: root-authored production parser-control admission changes in `application/projection/admission.rs`, `coordinator.rs`, domain `projection/options.rs`, and the bundled-helper input open flags in `infrastructure/process/image.rs`. The reviewer authored separate coverage-driven acceptance tests (admission rollback, completion facts and replay release) and excludes those tests from this independent verdict. This is static contract inspection only; no build, test rerun, workload or native investigation was performed. Exact reviewed hashes are in `docs/verification/control-admission-ddd-review/source.json`.

**No P1/P2 DDD/dependency-contract findings in the reviewed production changes.** Behavioral and organization reviews remain separately owned; this review does not claim those activities or gate completion.

## Parser-control admission

The domain still owns portable validated capacity choices and documented semantics: `staging_slots` bounds parser output and `request_slots` bounds controls/observations, including retained results. No executor, native, serialization or OS dependency enters domain/application through this change.

Application owns admission and ordering. Nonempty output obtains paired global/local parser slot and byte leases before copying/publication. Empty output returns Accepted without queueing. Every actual `staging(0)` caller—resize, view, checkpoint, begin_transfer—acquires its paired global/local request ticket first. Thus skipping a second parser-slot lease does not create uncharged control work. A dropped wait only marks its ticket cancelled; the queued/in-flight event still owns its Arc and lease. Completed retained waits continue holding request capacity until drop.

Controls and output continue using the same FIFO. The creation path now checks `staging_slots + request_slots` for overflow and reserves that combined finite queue capacity before accepting work. In-flight/requeued events retain their original leases, and request-capacity limits can therefore remain independent of parser pressure without unbounded queue growth. The existing lease RAII releases a global reservation if a later local reservation fails. These details support the split's dependency/ownership contract; they do not establish a new latency SLA.

Inspected independent `control_capacity.rs` tests exercise local/shared parser saturation, control request exhaustion, exact feed/resize ordering and retained-result quota ownership. This reviewer did not author or rerun those tests.

## Bundled helper open flags

`O_NONBLOCK` is added only at the infrastructure boundary that opens a caller-specified bundled helper source. It lets unsupported FIFO input reach the already-existing descriptor-metadata regular-file check without waiting for a writer. `O_NOFOLLOW` and `O_CLOEXEC` remain present. The code checks metadata on the opened descriptor, reads at most IMAGE.len()+1, requires exact embedded-image bytes, and stages the embedded image into a separately created private executable. The new input descriptor flag does not propagate to the staged executable or runtime process session.

All OS error mapping remains in infrastructure and exposes existing portable ProcessError values; invalid file type is InvalidCommand and content mismatch is Unsupported. No arbitrary bundled executable is admitted and no filesystem DTO leaks into application/domain. The dedicated tests cover exact bytes with source removal, altered/oversized bytes, links/directories and a bounded FIFO probe; no independent execution is claimed here.

## Documentation observation resolved by independent static recheck

P3 resolved: root corrected `crates/application/src/diagnostics/resources.rs:35` to describe parser output slots independently from controls charged to requests, and the `requests` comment now explicitly includes queued controls, pending operations and consumer-retained results. Independent static recheck confirms both comments agree with the actual paired-lease ownership. No remaining findings in this scoped DDD/dependency review. The reviewer changed documentation only and did not rerun tests or the gate.
