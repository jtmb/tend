#!/usr/bin/env python3
"""Upgrade only Tend binaries/assets. Never touch timers, settings, or reboot hooks."""
import hashlib
import os
import pwd
import re
import sys
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
            root_group_directory = stat.S_ISDIR(info.st_mode) and info.st_gid == 0 and not info.st_mode & 0o002
            if stat.S_ISLNK(info.st_mode) or info.st_uid != 0 or (info.st_mode & 0o022 and not root_group_directory):
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

if '--remember-authorization' in sys.argv[1:]:
    uid = int(os.environ.get('PKEXEC_UID', os.environ.get('SUDO_UID', '-1')))
    if uid < 1000:
        raise SystemExit('Run authorization setup through the desktop user.')
    user = pwd.getpwuid(uid).pw_name
    if not re.fullmatch(r'[a-zA-Z_][a-zA-Z0-9_-]*', user):
        raise SystemExit('Unsupported user name.')
    rule = Path('/etc/polkit-1/rules.d/49-tend-local-user.rules')
    body = '\n'.join([
        '// Explicit local authorization for the installed Tend helper. No password is stored.',
        'polkit.addRule(function(action, subject) {',
        '    if (action.id === "io.github.jtmb.Tend.manage" &&',
        '        action.lookup("program") === "/usr/local/libexec/tend-admin" &&',
        '        subject.user === ' + __import__('json').dumps(user) + ' && subject.local && subject.active) {',
        '        return polkit.Result.YES;',
        '    }',
        '});', ''
    ])
    if rule.exists():
        shutil.copy2(rule, rule.with_name(rule.name + '.backup.' + stamp))
    fd, temporary = tempfile.mkstemp(prefix='.tend-auth-', dir=rule.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            stream.write(body); stream.flush(); os.fsync(stream.fileno()); os.fchmod(stream.fileno(), 0o644)
        os.replace(temporary, rule)
    finally:
        Path(temporary).unlink(missing_ok=True)
    print('Tend authorization saved for ' + user + '. No password stored. Other administrator actions still require authentication.')
