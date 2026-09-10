#!/bin/bash
set -euo pipefail
cd /home/user/Documents/pty-runtime-spawn-diagnostic6
tar -xzf ../image-fork-green.tar.gz
python3 scripts/record_validation.py --output docs/verification/image-fork/green -- cargo test -p pty-runtime-infrastructure --lib constructor_image_executes_while_unrelated_child_is_between_fork_and_exec -- --nocapture
python3 scripts/record_validation.py --output docs/verification/image-fork/clippy -- cargo clippy -p pty-runtime-infrastructure --all-targets -- -D warnings
