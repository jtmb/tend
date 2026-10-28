"""Read-only preflight for a compatible, root-owned Tend integration."""
import json
import os
from pathlib import Path
import stat
import subprocess

COMPONENTS = (
    ('/usr/local/libexec/tend-admin', 'administrator', False),
    ('/usr/local/libexec/tend-worker', 'worker', False),
    ('/usr/local/lib/dev-maintenance/tend_cleanup_policy.py', 'maintenance-adapter', True),
    ('/usr/local/lib/dev-maintenance/tend_maintenance_history.py', 'maintenance-history', True),
)
MARKERS = ('/etc/tend/cleanup-policy-ready', '/etc/tend/exclusions-ready', '/etc/tend/history-ready')


def validate_capabilities(value, role):
    if not isinstance(value, dict) or type(value.get('protocol')) is not int or value['protocol'] < 4 or value.get('role') != role:
        raise ValueError('Incompatible Tend component: ' + role)
    flags = ('maintenance_events',) if role == 'maintenance-history' else ('cleanup_exclusions', 'explained_cleanup', 'maintenance_events')
    if any(value.get(flag) is not True for flag in flags):
        raise ValueError('Required integration support is missing: ' + role)


def trusted(path, owner=0):
    path = Path(path)
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != owner or info.st_mode & 0o022:
        raise ValueError('Untrusted Tend integration file: ' + str(path))
    for parent in path.parents:
        info = parent.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != owner or info.st_mode & 0o002 or (info.st_mode & 0o020 and info.st_gid != 0):
            raise ValueError('Untrusted Tend integration directory: ' + str(parent))


def check_integration():
    for name, role, python in COMPONENTS:
        path = Path(name)
        trusted(path)
        command = ['/usr/bin/python3', str(path), '--capabilities'] if python else [str(path), 'capabilities']
        result = subprocess.run(command, stdin=subprocess.DEVNULL, capture_output=True, timeout=3, check=False)
        if result.returncode != 0 or len(result.stdout) > 8192:
            raise ValueError('Tend integration upgrade is required: ' + role)
        validate_capabilities(json.loads(result.stdout), role)
    for name in MARKERS:
        path = Path(name)
        trusted(path)
        if path.stat().st_size > 8 or path.read_bytes().strip() != b'1':
            raise ValueError('Tend integration is incomplete: ' + name)
