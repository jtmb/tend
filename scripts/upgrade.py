#!/usr/bin/env python3
"""Upgrade only Tend binaries/assets. Never touch timers, settings, or reboot hooks."""
import hashlib
import os
import shutil
import stat
import tempfile
from datetime import datetime, timezone
from pathlib import Path

if os.geteuid() != 0:
    raise SystemExit('Administrator authentication is required to replace the installed app.')
os.umask(0o022)
root = Path(__file__).resolve().parents[1]
files = {
    'target/release/tend': '/usr/local/bin/tend',
    'target/release/tend-admin': '/usr/local/libexec/tend-admin',
    'target/release/tend-worker': '/usr/local/libexec/tend-worker',
    'assets/io.github.jtmb.Tend.policy': '/usr/share/polkit-1/actions/io.github.jtmb.Tend.policy',
    'assets/io.github.jtmb.Tend.desktop': '/usr/local/share/applications/io.github.jtmb.Tend.desktop',
    'assets/io.github.jtmb.Tend.svg': '/usr/local/share/icons/hicolor/scalable/apps/io.github.jtmb.Tend.svg',
}
for src, target in files.items():
    if not (root / src).is_file():
        raise SystemExit(f'Build the release binaries first: {src}')
    path = Path(target)
    for parent in (path, *path.parents):
        if parent.exists() or parent.is_symlink():
            info = parent.lstat()
            if stat.S_ISLNK(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
                raise SystemExit(f'Unprotected installation destination: {parent}')

stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
backups = {}
changed = []
try:
    for src, target in files.items():
        path = Path(target)
        path.parent.mkdir(parents=True, exist_ok=True, mode=0o755)
        if path.exists():
            backup = path.with_name(path.name + '.backup.' + stamp)
            shutil.copy2(path, backup)
            backups[path] = backup
        else:
            backups[path] = None
        fd, temporary = tempfile.mkstemp(prefix='.tend-upgrade-', dir=path.parent)
        try:
            with os.fdopen(fd, 'wb') as stream, (root / src).open('rb') as source:
                shutil.copyfileobj(source, stream)
                stream.flush()
                os.fsync(stream.fileno())
                os.fchmod(stream.fileno(), 0o755 if src.startswith('target/') else 0o644)
            os.replace(temporary, path)
            changed.append(path)
        finally:
            Path(temporary).unlink(missing_ok=True)
        if hashlib.sha256(path.read_bytes()).digest() != hashlib.sha256((root/src).read_bytes()).digest():
            raise RuntimeError(f'Installed checksum mismatch: {path}')
except BaseException:
    for path in reversed(changed):
        if backups[path] is None:
            path.unlink(missing_ok=True)
        else:
            os.replace(backups[path], path)
    raise
print('Tend upgraded. Binaries verified. No timers, maintenance settings, or reboot hooks changed.')
