#!/usr/bin/env python3
"""Mandatory repository gate; run from any working directory."""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def run(*args, env=None):
    print('+ ' + ' '.join(args), flush=True)
    subprocess.run(args, cwd=ROOT, env=env, check=True)


def architecture():
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], cwd=ROOT))
    locations = {
        'pty-runtime': ROOT / 'Cargo.toml',
        'pty-runtime-domain': ROOT / 'crates/domain/Cargo.toml',
        'pty-runtime-application': ROOT / 'crates/application/Cargo.toml',
        'pty-runtime-infrastructure': ROOT / 'crates/infrastructure/Cargo.toml',
    }
    packages = {p['name']: p for p in metadata['packages']}
    for name, manifest in locations.items():
        package = packages.get(name)
        if package is None or Path(package['manifest_path']).resolve() != manifest.resolve():
            raise SystemExit(f'Missing or relocated required package: {name}')
        if package['id'] not in metadata['workspace_members']:
            raise SystemExit(f'Required package is not a workspace member: {name}')
    allowed = {
        'pty-runtime-domain': set(),
        'pty-runtime-application': {'pty-runtime-domain'},
        'pty-runtime-infrastructure': {'pty-runtime-domain', 'pty-runtime-application'},
        'pty-runtime': {'pty-runtime-domain', 'pty-runtime-application', 'pty-runtime-infrastructure'},
    }
    for name, package in packages.items():
        for dep in package['dependencies']:
            dep_name = dep['name']
            if name in ('pty-runtime-domain', 'pty-runtime-application') and dep_name not in allowed[name]:
                raise SystemExit(f'Forbidden dependency in {name}: {dep_name}')
            if dep_name in locations:
                if dep_name not in allowed.get(name, set()):
                    raise SystemExit(f'Reversed workspace dependency: {name} -> {dep_name}')
                if dep.get('path') is None or Path(dep['path']).resolve() != locations[dep_name].parent.resolve():
                    raise SystemExit(f'Workspace dependency points elsewhere: {name} -> {dep_name}')
    for folder in ['src', 'crates', 'scripts/native', 'tests']:
        for path in (ROOT / folder).rglob('*'):
            if path.suffix not in ('.rs', '.c', '.h'):
                continue
            count = sum(bool(line.strip()) for line in path.read_text().splitlines())
            if count > 350:
                raise SystemExit(f'{path.relative_to(ROOT)}: {count} nonblank lines exceeds 350')


def main():
    architecture()
    run('python3', '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v')
    for package in ['pty-runtime-domain', 'pty-runtime-application']:
        run('cargo', 'test', '--locked', '-p', package, '--no-default-features')
    run('cargo', 'fmt', '--all', '--check')
    run('python3', 'scripts/native/bootstrap.py')
    run('cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--all-features', '--', '-D', 'warnings')
    run('cargo', 'test', '--locked', '--workspace', '--all-targets', '--all-features')
    run('cargo', 'test', '--locked', '--workspace', '--no-default-features')
    run('cargo', 'doc', '--locked', '--workspace', '--no-deps', '--all-features',
        env={**os.environ, 'RUSTDOCFLAGS': '-D warnings'})
    run('python3', '-m', 'unittest', 'discover', '-s', 'experiments/tests', '-v')
    run('cargo', 'fmt', '--manifest-path', 'experiments/pty/Cargo.toml', '--check')
    run('cargo', 'clippy', '--manifest-path', 'experiments/pty/Cargo.toml', '--locked', '--all-targets', '--', '-D', 'warnings')
    print('Mechanical gate passed. Specialist reviews and milestone proof remain required.')


if __name__ == '__main__':
    main()
