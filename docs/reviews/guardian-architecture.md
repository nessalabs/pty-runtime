# Guardian implementation architecture review

Reviewed 2026-09-08. Independent production source review of the packaged helper,
shared protocol/build hook, and infrastructure launch, registration, ownership,
exit-watch and lifecycle adapters. This report supplements
`guardian-production-design-review.md`; it does not close its production
qualification requirements or substitute for the runtime gate.

## Result

No remaining concrete P1/P2 DDD, SOLID or Clean Code finding in the reviewed
implementation after the two ownership fixes below. The implementation preserves
inward dependencies: the standalone helper depends only on libc and a shared
wire codec; the infrastructure adapter translates helper observations into
application process events and domain errors. Domain/application code does not
acquire OS descriptor, PID-discovery or executable-materialization policy.

The helper responsibilities are cohesive: discovery supplies error-aware
candidates, anchors establish signal authority, cancellation manages normal
foreground escalation, Sweep retains emergency cleanup ownership, Links manages
bounded status delivery, and S/G/successor modules own topology transitions.
Keeping this OS ownership machinery outside domain lifecycle policy is
appropriate. Concrete small types avoid a speculative process-control framework.

Actual W wait status remains distinct from S/G status. The host validates the
handshake and generation, retains both control channels and the direct S child,
and reports supervision loss independently. Registration and launch failures
have a Guardian cleanup owner. PTY reading and discarded draining remain separate
from helper completion, avoiding terminal-output teardown deadlock.

## Resolved findings

1. **P2: anchor creation could lose its child ledger on transport setup failure.**
   `Anchor::start` originally forked before the fallible parent `Channel::new`.
   Failure could return without an Anchor retaining the child PID for wait;
   repeated failed setup could accumulate unreaped children. The author moved
   transport setup before fork. After fork, optional exit-watch failure retains
   the Anchor and uses wait polling. Verified in current source.
2. **P1: failed successor descriptor inventory could suppress owner/peer EOF.**
   The former guardian originally ignored `close_unrelated` failure before
   parking on its private monitor. It could retain duplicate control endpoints;
   if the successor then died, the sentinel could fail to observe EOF. The author
   now terminates the former guardian's actual group immediately on that error,
   allowing the kernel to close every copied endpoint and sentinel recovery to
   continue. No numeric PID/PGID fallback was introduced. Verified in current
   source.

## Bounds and ownership observations

The wire format is fixed and versioned, with one partial incoming record and
at most eight queued outgoing records per channel. Discovery is resumable and
unknown/truncated observations retain cleanup rather than prove emptiness.
The anchor verifies its own SID after joining, and signals only its current
group. The direct workload PID is signalled only while its parent retains the
unreaped child identity. Helper-group entrants have explicit retirement and
successor handling. Platform-specific process inventories are isolated from
signalling authority.

Host spawn setup uses a fresh executable and syscall-only pre-exec descriptor
mapping. Helper image materialization is infrastructure-owned, exclusive, private
and RAII-cleaned; the build hook validates target image format/architecture.
These code boundaries are sound, but they do not themselves qualify signed
macOS distribution, resource ceilings or fault-recovery behavior on both OSes.

## Validation and remaining qualification

Independently ran `cargo test --manifest-path helpers/guardian/Cargo.toml --locked
--bin pty-runtime-guardian`: all three protocol tests passed (fragmentation,
stale-generation/partial EOF rejection and bounded outgoing records). The author
reported the macOS process contract suite passing after the fixes; that is
reported evidence, not an independent rerun in this review.

The parent task must still evaluate packaged macOS/Linux fault cases, injected
resource/discovery failures, helper-group entrants, owner EOF, partial startup,
status backpressure, signed packaging and complete 1/32/128-session resource
measurements against the existing design acceptance checklist. Architectural
review success alone is not evidence for G1 closure or unchanged capacity claims.
