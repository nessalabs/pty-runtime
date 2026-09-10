#!/bin/bash
set -euo pipefail
cd /home/user/Documents
git clone --local pty-runtime-candidate6 pty-runtime-spawn-diagnostic6
cd pty-runtime-spawn-diagnostic6
cp -a ../pty-runtime-candidate6/target target
python3 - <<'PY'
from pathlib import Path
p=Path('crates/infrastructure/src/process/mod.rs')
s=p.read_text().replace('fn error(error: std::io::Error) -> ProcessError {','fn error(error: std::io::Error) -> ProcessError {\n    eprintln!("spawn_diagnostic stage=io_conversion kind={:?} errno={:?}", error.kind(), error.raw_os_error());')
p.write_text(s)
p=Path('crates/infrastructure/src/process/spawn.rs')
s=p.read_text().replace('let sentinel = command.spawn().map_err(error)?;','let sentinel = command.spawn().map_err(|failure| {\n        eprintln!("spawn_diagnostic stage=initial_guardian kind={:?} errno={:?}", failure.kind(), failure.raw_os_error());\n        error(failure)\n    })?;')
p.write_text(s)
PY
python3 scripts/record_validation.py --output docs/verification/spawn-diagnostic6/build -- cargo test --locked --no-default-features --features event-stream --test raw_runtime --no-run
