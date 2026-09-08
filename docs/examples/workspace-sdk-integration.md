# Workspace-sdk integration sketch

This is a source-backed adapter design for ADR 0001 section 7, not an implemented
workspace-sdk integration or a claim that pty-runtime is release-ready. The sibling
package was inspected without modification. Its selected source hashes are in
`../verification/workspace-sdk-sketch/source.json`; that directory has no Git
metadata, so no sibling revision is inferred.

## Current boundary and required host lifetime

`src/runtime/backend_host.rs::BackendRuntimeHost::start_interactive` currently
rejects `requires_terminal`; ordinary managed runs use `IBackend::start` or
`start_private`. `IRuntimeHost` already declares output, input, resize, inspect
and cancel operations. The inspected `IBackend` does not yet expose terminal
start/input/resize operations, and `ManagedBackend` sends one typed supervisor
request through its target transport.

`src/supervisor/cli.rs::run_cli` handles one request in a dedicated single-threaded
process. Constructing a PTY `Runtime` inside that request would terminate its
sessions when the request ends. The subsequent integration therefore needs a
long-lived target-side owner with one runtime and a bounded authenticated control
endpoint. Short-lived transport invocations reconnect to that owner. A local
client's runtime must never substitute for a remote workspace's target runtime.
Selecting/deploying that owner and extending the SDK transport are sibling-package
work, outside this library delivery.

Keep existing noninteractive execution, private-file provisioning, resource
inspection and authentication-status checks on their current backend paths. The
new terminal capability should default to `Unsupported` for providers that cannot
reach the target owner. A raw PTY is sufficient for the authentication transcript;
use projection only when a consumer explicitly needs terminal state.

## Operation mapping

| SDK boundary | Proposed PTY adapter behavior |
| --- | --- |
| `start_interactive(key, command, requires_terminal)` | Validate target, roots and command before I/O. Derive a bounded `SessionId` from the scoped attempt key; preserve literal argv and explicit environment removals/overrides. Use `Runtime::spawn` once, then `lookup` for the same attempt. A concurrent duplicate must reconnect to the existing admission result, not launch again. Reject changed parameters for an existing immutable attempt. |
| `RuntimeProcessRef` | Keep the exact SDK host scope. Use an opaque adapter reference bound to the PTY session lifetime as well as its registry ID; never use a PID. The current SDK reference has no separate lifetime field, so the adapter must encode/bind it in its opaque ID or bounded owner registry. An old reference must never address a newly reused ID. |
| `inspect_process` | Read `Session::status`. Known `ExitStatus::Code` supplies the actual exit code. Signal exit requires an explicit SDK representation or documented signal-to-code convention. Admission/supervision uncertainty and owner loss map to `Lost`; timeout/cancel admission must not manufacture an exit. Exit zero still requires the SDK's separate native authentication-status check. |
| `read_process_output` | Validate scope, lifetime, cursor and the SDK limit before reading. Attach at `ReplayCursor { lifetime, offset: cursor.stdout }`; place merged PTY bytes in stdout and require stderr cursor zero. A raw replay gap sets `OutputPage::gap` and advances the absolute raw position. Lossy UTF-8 text length is not the byte cursor. |
| `write_process_input` | Submit transient secret bytes through `Session::write`, then await its operation. An admission error can be returned as an SDK error only when no bytes were admitted. A known written prefix maps to `Accepted { bytes }`; lost acknowledgement/transport or a cancelled wait must remain ambiguous, never become a safe whole-input retry. Do not log or persist the input. |
| `resize_process` | Convert the validated SDK size through `TerminalSize::new`. Await raw `Session::resize`; for a deliberately projected session use `resize_projected` and preserve its separate OS/model outcomes rather than claiming success after one side fails. |
| `cancel_process` | Admit `Session::cancel`, then inspect or wait under the caller's deadline. A cancelled transport future does not revoke cancellation. Return only observed state and preserve the same reference for reconciliation. |
| Disconnect/reconnect | Drop the attachment or pending read, retain the owner/session and caller-acknowledged cursor. Reconnect through lookup, never by respawning the command. |
| Owner shutdown | Reject new starts, call `Runtime::shutdown`, then release injected services. SDK receipts cannot promise process survival after owner restart; absent lifetimes are lost, not automatically recreated. |

The runtime's fixed replay-page size can exceed an individual SDK read limit.
An adapter may take a prefix from an immediate replay page and return the cursor
for exactly that prefix; a later request reattaches at that returned cursor. It
must not expose the attachment's farther internal cursor after truncating the
page. Keep the temporary page bounded by the runtime configuration. A gap-only or
currently pending read may return an empty, incomplete page without polling until
process exit. `OutputPage::complete` requires consumed output and confirmed
completion; truncated/failed drain must remain explicit error or incomplete-loss
state, not a clean complete transcript.

The SDK accepts larger input submissions than the runtime's default per-chunk
limit. Configure finite compatible limits or explicitly chunk a submission while
tracking its acknowledged prefix. Do not silently relax bounds, repeat a prefix,
or claim that OS acceptance proves the child consumed the bytes. The SDK's two-
variant input outcome also cannot carry every PTY error detail; preserve delivery
certainty first and report process failure through the separate state path.

## Applicability and follow-up proof

The public PTY API accepts already-encoded bytes; it exposes no key/mouse-to-byte
encoding operation. Such bytes may be written while the model is parked, and
resulting PTY output follows normal restoration. Any future SDK/model-dependent
encoding operation must explicitly restore/query the model before encoding. This
sketch does not invent that additional API or declare the parked-resize proof
complete.

The sibling integration must verify same-key concurrency, mismatched-attempt
rejection, foreign/old references, raw-byte page limits and UTF-8 boundaries,
partial/unknown input without replay, resize/cancel during exit, disconnect
retention, and owner restart/loss. Use a synthetic authentication fixture and a
separate status-check command. Existing pty-runtime tests establish their recorded
library scope; they do not constitute execution of this proposed SDK adapter.
