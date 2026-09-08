# CI for 5373d8e

Both the runtime and PTY experiment workflows completed successfully for
5373d8e6f0f263b8a25d7dc2b22ba3bf841b9bef. The runtime workflow covers macOS,
Linux and MSRV; the experiment workflow covers macOS and Linux. Exact job
status and URLs are retained in the JSON records. This verifies that the
mixed-memory/pool validator correction fixes the prior CI failure.

These workflows qualify their stated checks only. The independently executed
projected-capacity trials fail at resize admission, and release qualification
remains incomplete. Later working-tree changes are not covered by this CI.
