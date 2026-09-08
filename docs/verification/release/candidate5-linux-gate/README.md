# Candidate5 Linux gate and build

Both commands passed with unchanged source, at revision 71c1d6f42bbd1c547f747fdd7d20ed69e3000c4e. The 293-file inventory exactly matches the macOS candidate5 release build and reviewed macOS gate. The full 12-hour soak began on this revision at 2026-09-08T21:15:08Z but failed after about146seconds with Projection(Worker), before completion. Its raw record is retained in ../candidate5-soak-failed.jsonl; cause remains under Rust-side diagnosis. Linux source directory /home/user/Documents/pty-runtime-candidate5 is frozen for this run.
