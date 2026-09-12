# pty-runtime

Private Rust workspace implementing bounded Unix PTY sessions and replaceable
headless terminal projection. The initial reader strategy is one dedicated
reader per live PTY. Implementation and release qualification are in progress;
standalone experiment results are not evidence of a finished session runtime.

Read [embedding guidance](docs/usage.md), the [docs hub](docs/README.md),
[what we have verified](docs/verification.md), [coding standards](coding_standards.md),
and [experiment instructions](experiments/README.md).

The workspace separates domain, application ports/use cases, infrastructure
adapters, and the public `pty-runtime` facade. Run `python3 scripts/gate.py` for
the mandatory mechanical gate. Product behavior lives in `docs/features/`.

Intended platforms are macOS and Linux on arm64 and x86_64. **No platform is
claimed as supported yet**, because none has completed qualification: macOS arm64
and Linux x86_64 are the candidates, having been exercised by standalone
experiment fixtures, and **macOS x86_64 and Linux arm64 have no executed evidence
at all**. See the
[platform table](docs/README.md#platform) and
[ADR 0004](docs/adr/0004-integration-and-release-qualification.md#platform-qualification).

Native prerequisites are Python 3.9+, a C compiler, and Rust.
`python3 scripts/native/bootstrap.py` downloads SHA-verified Zig/Ghostty pins and
builds the native library into the ignored cache. The mandatory gate prepares
that cache automatically. `cargo test --no-default-features` exercises raw-only
use without Ghostty; projected use requires the default `ghostty` feature.
