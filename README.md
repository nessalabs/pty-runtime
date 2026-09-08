# pty-runtime

Private Rust workspace implementing bounded Unix PTY sessions and replaceable
headless terminal projection. The initial reader strategy is one dedicated
reader per live PTY. Implementation and release qualification are in progress;
standalone experiment results are not evidence of a finished session runtime.

Read [the ADRs](docs/README.md), [coding standards](coding_standards.md), and
[experiment instructions](experiments/README.md).

The workspace separates domain, application ports/use cases, infrastructure
adapters, and the public `pty-runtime` facade. Run `python3 scripts/gate.py` for
the mandatory mechanical gate; specialist review records live in `docs/reviews`
and requirement evidence in `docs/verification`.

Native prerequisites are Python 3.9+, a C compiler, and Rust.
`python3 scripts/native/bootstrap.py` downloads SHA-verified Zig/Ghostty pins and
builds the native library into the ignored cache. The mandatory gate prepares
that cache automatically. `cargo test --no-default-features` exercises raw-only
use without Ghostty; projected use requires the default `ghostty` feature.
