# Helper fault recovery: independent adversarial review

Reviewer: code_quality_gate, independent of helper implementation author. Result: scoped PASS; no unresolved P1/P2 findings. This is not full quality-gate acceptance or a 100% coverage claim.

## Scope and evidence

Reviewed the eight sources listed in `work/helper-fault-recovery/source-and-production.json`: guardian Cargo target declaration, main module registration, discovery/anchor/successor injection sites, and the three new fault fixture modules. Read the real cleanup, cancellation, anchor, and successor ownership paths to check the assertions against actual behavior. Independently verified all eight source hashes and the production binary hash against that manifest.

Supplied evidence inspected: `all-targets.log` passes 17 helper unit tests, two startup tests, and all four single-threaded real-process fault scenarios. `clippy.log` passes. `production-build.log` and the manifest record a production build with no fault-injection symbol. No new build was launched during this review.

## Adversarial checks

- Failure injection is guarded by `cfg(test)` at all three production call sites. TLS state has a process identity check, finite consumption count, non-nesting assertion, and a guard that cannot move across threads. Forked children cannot consume the parent's arm.
- The harness-free executable creates no test-runner worker threads. It creates its own PTY session and checks distinct inherited/current groups before exercising real child ownership. Readiness confirms the shell installed its signal disposition, and actual child SID, PGID, and foreground identity are asserted before signals.
- Discovery and anchor failures preserve a live workload and the 50 ms retry state. Subsequent real cleanup must observe SIGKILL and retirement without reserved members. The discovery test also checks no retry at 49 ms.
- Failed handoff retains both healthy channels, sends actual fault records with expected payloads, preserves the live workload, and then completes a real cleanup sweep.
- Missing cancellation anchors consume both requested faults, suppress false TERM/escalation acknowledgements, retain the full grace interval, and require actual SIGKILL completion. The escalation acknowledgement is consumed exactly once.
- Failure/unwind cleanup closes anchor channels, kills and reaps the directly owned workload with a bounded deadline, and then waits for all remaining children to be gone. Cleanup failures fail the executable rather than being counted as successful scenarios.

## Findings, fixes, and remaining limitations

No change-blocking findings or requested fixes from this review. Existing first-attempt compile evidence remains preserved in the evidence directory.

These are operation-entry failures, not individual syscall or partial-setup injection. In particular, the handoff scenario invokes the retain/fault composition explicitly after the injected error; it does not drive the guardian/successor event loop through every topology branch. The tests establish the shown recovery contracts, not child-side setup failure handling, allocation failure recovery, all concurrent process topologies, or a performance bound beyond the fixture deadlines. The evidence was collected on the current macOS host and does not establish a Linux run. Full mechanical/performance acceptance and the separate code-quality review remain the owning task's responsibility.
