#!/bin/bash
set -euo pipefail
cd /home/user/Documents/pty-runtime-spawn-diagnostic6
python3 scripts/record_validation.py --output docs/verification/image-fork/fixed-raw-build -- cargo test --locked --no-default-features --features event-stream --test raw_runtime --no-run
python3 scripts/record_validation.py --output docs/verification/image-fork/fixed-raw-repeats -- python3 /home/user/Documents/untraced-fixed-spawn.py
