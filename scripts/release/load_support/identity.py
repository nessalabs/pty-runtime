"""Content identities are retained independently of a dirty source revision label."""
import hashlib
import platform
import subprocess
from pathlib import Path

def command(args):
    try:
        result = subprocess.run(args, text=True, capture_output=True)
    except FileNotFoundError:
        return dict(command=args, exit_code=None, stdout=None, stderr="unavailable")
    return dict(command=args, exit_code=result.returncode, stdout=result.stdout.strip(), stderr=result.stderr.strip())

def identify(binary):
    names = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard'], text=True)
    inventory = []
    for name in sorted(set(names.splitlines())):
        path = Path(name)
        if path.is_file() and path.suffix in ('.rs', '.zig', '.toml', '.lock', '.py', '.c', '.h'):
            inventory.append(dict(path=name, sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    dirty = subprocess.check_output(['git', 'diff', '--binary'])
    return dict(event='identity', platform=platform.platform(), machine=platform.machine(),
                source_head=command(['git', 'rev-parse', 'HEAD'])['stdout'],
                diff_sha256=hashlib.sha256(dirty).hexdigest(), source_inventory=inventory,
                binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                tools=[command(['rustc', '-Vv']), command(['cargo', '-V']), command(['zig', 'version'])],
                host=command(['sysctl', 'hw.model', 'hw.ncpu', 'hw.memsize', 'machdep.cpu.brand_string']) if platform.system() == 'Darwin' else command(['lscpu']),
                source_binary_link='Caller must retain build log for this inventory; hashes alone do not prove which source built binary.')
