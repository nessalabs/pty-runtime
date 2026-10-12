# PR 27 permission fixture: independent DDD review

Scoped PASS; no P1/P2 ownership or dependency-direction finding. Reviewed the
single test-only diff at HEAD `e729f1c07096b4745eda18c6bf90c9ce282018cc` and the
existing syscall fault/report path. Reviewed fixture SHA-256:
`36dbdc0ce4a6b8b85ca2b2584ba380d70d6c441a6cd3c0b6727ba280feb3edb0`.
No build, test or source edit was performed by this reviewer.

The corrected test owns a materializer error/recovery contract, not the host's
DAC policy. It now requests exactly one Open result of -1/EACCES through the
already authorized cfg(test) seam and requires exactly one F receipt. The real
materializer child, error translation, reap and parent signal-mask restoration
still execute. No fabricated successful PID/wait result or new production seam
is introduced. The injection remains an infrastructure test concern and does
not add a domain/application dependency on the fixture.

The important effect assertions remain exact: PermissionDenied, unchanged caller
mask, no destination inode, unchanged existing sentinel contents, and successful
unfaulted retry with exact bytes and unchanged mask. Removing chmod-based denial
eliminates a UID-0/capability assumption. It does change the evidence scope:
this test no longer proves that a real filesystem denied this process permission.
Its new name and comment state that narrower simulated-EACCES scope explicitly;
they do not claim injected failure is observed OS permission enforcement.

The single consumption receipt prevents a passing result from being attributed
to an unrelated error. Reusing the existing per-call scalar plan preserves its
fork constraints and resource ownership; the test adds no global switch, private
poisoning, skipped assertion or coverage filter. The unmodified normal write on
retry proves the fault did not persist beyond the one call.

This is static source acceptance. Fresh focused execution, independent quality
and adversarial reviews and required mechanical/performance gates remain needed.
The user's coverage waiver applies only to coverage and does not waive those
other gates or convert this review into a completed runtime verification.
