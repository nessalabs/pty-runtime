#!/usr/bin/env python3
"""Record a command with source identity; reject evidence if sources change mid-run."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def source_manifest():
    files = [ROOT / name for name in ('Cargo.toml', 'Cargo.lock', 'coding_standards.md', 'AGENTS.md')]
    for folder in ['src', 'crates', 'scripts', 'tests', '.github', 'experiments']:
        files.extend(path for path in (ROOT / folder).rglob('*')
                     if path.is_file() and not {'target', '__pycache__'} & set(path.parts)
                     and path.suffix in {'.rs', '.c', '.h', '.py', '.toml', '.lock', '.yml', '.yaml'} )
    return {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(set(files))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command:
        parser.error('provide a command after --')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    before = source_manifest()
    started = time.time()
    with (output / 'command.log').open('w') as log:
        result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
    after = source_manifest()
    metadata = {
        'command': command, 'started_unix': started, 'elapsed_seconds': time.time() - started,
        'platform': platform.platform(), 'architecture': platform.machine(),
        'rustc': subprocess.check_output(['rustc', '-Vv'], cwd=ROOT, text=True),
        'base_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'exit_code': result.returncode, 'sources_unchanged': before == after,
        'source_files': before,
        'passed': result.returncode == 0 and before == after,
    }
    (output / 'metadata.json').write_text(json.dumps(metadata, indent=2, sort_keys=True) + '\n')
    if before != after:
        (output / 'source_files_after.json').write_text(json.dumps(after, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'passed': metadata['passed'], 'exit_code': result.returncode,
                      'sources_unchanged': before == after, 'output': str(output)}))
    raise SystemExit(0 if metadata['passed'] else 1)


if __name__ == '__main__':
    main()
