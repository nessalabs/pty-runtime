"""Content identities are retained independently of a dirty source revision label."""
import hashlib
import platform
import resource
import subprocess
from pathlib import Path

def command(args):
    try:
        result = subprocess.run(args, text=True, capture_output=True)
    except FileNotFoundError:
        return dict(command=args, exit_code=None, stdout=None, stderr="unavailable")
    return dict(command=args, exit_code=result.returncode, stdout=result.stdout.strip(), stderr=result.stderr.strip())

def descriptor_limit():
    """Descriptor limit in force for this driver, which every fixture inherits.

    The 128-session cases need more than the common 1024 default and fail at
    session spawn without it, before any measurement. Experiment 0005 recorded
    128-active both passing and failing to start on the same host because this
    was never captured.
    """
    soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
    return dict(soft=soft, hard=hard,
                unlimited=soft == resource.RLIM_INFINITY)


def identify(binary):
    names = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard'], text=True)
    inventory = []
    for name in sorted(set(names.splitlines())):
        path = Path(name)
        if path.is_file() and path.suffix in ('.rs', '.zig', '.toml', '.lock', '.py', '.c', '.h'):
            inventory.append(dict(path=name, sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    dirty = subprocess.check_output(['git', 'diff', '--binary'])
    return dict(event='identity', platform=platform.platform(), machine=platform.machine(),
                descriptor_limit=descriptor_limit(),
                source_head=command(['git', 'rev-parse', 'HEAD'])['stdout'],
                diff_sha256=hashlib.sha256(dirty).hexdigest(), source_inventory=inventory,
                binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                tools=[command(['rustc', '-Vv']), command(['cargo', '-V']), command(['zig', 'version'])],
                host=command(['sysctl', 'hw.model', 'hw.ncpu', 'hw.memsize', 'machdep.cpu.brand_string']) if platform.system() == 'Darwin' else command(['lscpu']),
                source_binary_link='Caller must retain build log for this inventory; hashes alone do not prove which source built binary.')
