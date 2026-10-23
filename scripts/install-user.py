#!/usr/bin/env python3
"""Install/update Tend's interface for the current user without administrator prompts."""
import hashlib
import os
import pwd
import re
import shutil
import tempfile
from datetime import datetime, timezone
from pathlib import Path

if os.geteuid() == 0:
    raise SystemExit('Run this installer as your desktop user, without sudo or pkexec.')
root = Path(__file__).resolve().parents[1]
home = Path(pwd.getpwuid(os.getuid()).pw_dir)
source = root / 'target/release/tend'
helper = Path('/usr/local/libexec/tend-admin')
if not source.is_file():
    raise SystemExit('Build first: cargo build --release --locked -j 1 --bin tend')
if not helper.is_file() or not os.access(helper, os.X_OK) or helper.stat().st_uid != 0 or helper.stat().st_mode & 0o022:
    raise SystemExit('The existing trusted Tend helper is required. No authentication prompt was opened.')
files = {
    home / 'scripts/tend': (source.read_bytes(), 0o755),
    home / '.local/share/icons/hicolor/scalable/apps/io.github.jtmb.Tend.svg': ((root/'assets/io.github.jtmb.Tend.svg').read_bytes(), 0o644),
}
binary = home / 'scripts/tend'
# Desktop Exec uses double quotes; escape characters with special meaning there.
quoted = str(binary).replace('\\', '\\\\').replace('"', '\\"').replace('$', '\\$').replace('`', '\\`')
command = str(binary) if re.fullmatch(r'[A-Za-z0-9_./-]+', str(binary)) else '"' + quoted + '"'
desktop = (root/'assets/io.github.jtmb.Tend.desktop').read_text().replace('Exec=/usr/local/bin/tend', 'Exec=' + command)
files[home / '.local/share/applications/io.github.jtmb.Tend.desktop'] = (desktop.encode(), 0o644)
stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
for path, (data, mode) in files.items():
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink():
        raise SystemExit('Refusing to replace a symbolic link: ' + str(path))
    if path.is_file() and path.read_bytes() == data:
        continue
    if path.exists():
        shutil.copy2(path, path.with_name(path.name + '.backup.' + stamp))
    fd, temporary = tempfile.mkstemp(prefix='.tend-user-', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(data); stream.flush(); os.fsync(stream.fileno()); os.fchmod(stream.fileno(), mode)
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)
    if hashlib.sha256(path.read_bytes()).digest() != hashlib.sha256(data).digest():
        raise SystemExit('Installed checksum mismatch: ' + str(path))
print('Tend interface installed for ' + home.name + '. Future interface updates need no password.')
print('The trusted system helper and its existing authorization are unchanged.')
