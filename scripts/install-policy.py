#!/usr/bin/env python3
"""Install only Tend's cleanup integration. Dry-run by default; never starts maintenance."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import tempfile
from datetime import datetime, timezone

REPO = Path(__file__).resolve().parents[1]
ENGINE = '/usr/local/lib/dev-maintenance/dev-maintenance.py'
MODULE = '/usr/local/lib/dev-maintenance/tend_cleanup_policy.py'
HELPER = '/usr/local/libexec/tend-admin'
WORKER = '/usr/local/libexec/tend-worker'
READY = '/etc/tend/cleanup-policy-ready'
TARGETS = (ENGINE, MODULE, HELPER, WORKER, READY)
MARKER = '# Tend cleanup integration v1'


def patch_engine(original):
    if MARKER in original:
        # Never duplicate integration or silently accept an unknown revision.
        if original.count(MARKER) != 1:
            raise ValueError('Unexpected existing cleanup integration')
        return original
    signature = 'def execute_phase(name, state, now=None):'
    wrapper = '''# Tend cleanup integration v1
import tend_cleanup_policy as _tend_cleanup

def execute_phase(name, state, now=None, cleanup_policy=None):
    if name in _tend_cleanup.PHASES:
        policy = _tend_cleanup.snapshot() if cleanup_policy is None else cleanup_policy
        return _tend_cleanup.execute(name, state, policy, run_command, as_user,
                                     apt_locks_busy, npm_busy, now)
    return _tend_legacy_execute_phase(name, state, now)


def _tend_legacy_execute_phase(name, state, now=None):'''
    replacements = {
        signature: wrapper,
        '        for name in PHASES:': '        cleanup_policy = _tend_cleanup.snapshot()\n        for name in PHASES:',
        'phase_status, message = execute_phase(name, state, now)': 'phase_status, message = execute_phase(name, state, now, cleanup_policy)',
        'def dry_run(state_dir):': 'def dry_run(state_dir):\n    policy = _tend_cleanup.snapshot()\n    print(_tend_cleanup.describe(policy))',
        '    print("APT: apt-get autoclean; no autoremove")': '    print("Policy above controls cleanup; updates retain their existing guards.")',
        '    print("Docker: docker builder prune --all --force --filter until=336h --keep-storage 5GB")': '',
        '    print("Docker: docker image prune --force --filter until=336h (dangling images only)")': '',
        '["/usr/bin/docker", "builder", "du", "--filter", "until=336h"]': '["/usr/bin/docker", "builder", "du", "--filter", "until={}h".format(policy["build_days"] * 24)]',
        '> 336 * 3600': '> policy["image_days"] * 86400',
        '"older than 14 days:"': '"older than {} days:".format(policy["image_days"])',
        '    print("npm: remove regular diagnostic logs older than 30 days; cache verify at most weekly")': '',
    }
    candidate = original
    for old, new in replacements.items():
        if candidate.count(old) != 1:
            raise ValueError('Installed engine differs from the supported layout: ' + old[:75])
        candidate = candidate.replace(old, new, 1)
    compile(candidate, ENGINE, 'exec')
    return candidate


def destination(root, path):
    return root / path.lstrip('/')


def protected(path, root):
    boundary = Path('/') if root == Path('/') else root
    owner = 0 if root == Path('/') else os.geteuid()
    for item in (path, *path.parents):
        if item.exists() or item.is_symlink():
            m = item.lstat()
            if stat.S_ISLNK(m.st_mode) or m.st_uid != owner or m.st_mode & 0o022:
                raise ValueError('Unprotected destination: ' + str(item))
        if item == boundary:
            break


def atomic(path, data, mode):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o755)
    fd, temporary = tempfile.mkstemp(prefix='.tend-policy-', dir=path.parent)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
        os.fchmod(stream.fileno(), mode)
    os.replace(temporary, path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true', help='Apply the reviewed installation; requires root for the real system')
    parser.add_argument('--dry-run', action='store_true', help='Explicit default; report only')
    parser.add_argument('--root', type=Path, default=Path('/'), help='Isolated staging root for validation')
    parser.add_argument('--rollback', type=Path, help='Restore an installer manifest, preserving displaced files as backups')
    args = parser.parse_args()
    if args.apply and args.dry_run:
        parser.error('Choose --apply or --dry-run')
    root = args.root.absolute()
    if root.is_symlink() or root.resolve() != root:
        raise SystemExit('Staging root must not contain symbolic links')
    if args.apply and root == Path('/') and os.geteuid() != 0:
        raise SystemExit('System installation needs administrator authentication. No prompt was opened.')
    lock = None
    if args.apply and root == Path('/'):
        lock_path=Path('/var/lib/dev-maintenance/run.lock')
        protected(lock_path,root)
        fd=os.open(lock_path,os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600)
        lock=os.fdopen(fd,'a')
        try:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit('Maintenance or another settings change is active; retry installation later.')
    if args.rollback:
        manifest = json.loads(args.rollback.read_text())
        if manifest.get('root') != str(root):
            raise SystemExit('Rollback root mismatch')
        changes = {}
        for entry in manifest['files']:
            if entry['path'] not in TARGETS:
                raise SystemExit('Unexpected rollback path')
            path = destination(root, entry['path'])
            protected(path, root)
            backup = Path(entry['backup']) if entry['backup'] else None
            if backup and (backup.parent != path.parent or not backup.name.startswith(path.name + '.backup.')):
                raise SystemExit('Invalid backup location')
            if backup:
                protected(backup, root)
            changes[path] = (backup.read_bytes(), entry['mode']) if backup else None
    else:
        engine = destination(root, ENGINE)
        original = engine.read_text()
        candidate = patch_engine(original)
        changes = {
            engine: (candidate.encode(), stat.S_IMODE(engine.stat().st_mode)),
            destination(root, MODULE): ((REPO/'assets/tend_cleanup_policy.py').read_bytes(), 0o644),
            destination(root, HELPER): ((REPO/'target/release/tend-admin').read_bytes(), 0o755),
            destination(root, WORKER): ((REPO/'target/release/tend-worker').read_bytes(), 0o755),
            destination(root, READY): (b'1\n', 0o644),
        }
        compile(changes[destination(root, MODULE)][0], MODULE, 'exec')
    for path in changes:
        protected(path, root)
        print(('APPLY ' if args.apply else 'DRY RUN ') + str(path))
    if not args.apply:
        print('No files changed. No authentication requested.')
        return
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
    manifest = {'root': str(root), 'files': []}
    applied = []
    try:
        # Ready marker is last: an incomplete installation never advertises support.
        for path, value in changes.items():
            exists = path.is_file()
            backup = path.with_name(path.name + '.backup.' + stamp) if exists else None
            old_mode = stat.S_IMODE(path.stat().st_mode) if exists else 0o644
            if backup:
                shutil.copy2(path, backup)
            manifest['files'].append({'path': '/' + str(path.relative_to(root)), 'backup': str(backup) if backup else None, 'mode': old_mode})
            if value is None:
                if exists:
                    os.replace(path, path.with_name(path.name + '.disabled.' + stamp))
            else:
                data, mode = value
                atomic(path, data, mode)
                if hashlib.sha256(path.read_bytes()).digest() != hashlib.sha256(data).digest():
                    raise RuntimeError('Checksum mismatch: ' + str(path))
            applied.append((path, backup))
    except BaseException:
        for path, backup in reversed(applied):
            if backup:
                atomic(path, backup.read_bytes(), stat.S_IMODE(backup.stat().st_mode))
            elif path.exists():
                os.replace(path, path.with_name(path.name + '.inactive.' + stamp))
        raise
    output = destination(root, '/etc/tend/policy-install.' + stamp + '.json')
    atomic(output, (json.dumps(manifest, indent=2) + '\n').encode(), 0o600)
    print('Verified installation. No timers, services or maintenance were invoked.')
    print('Rollback manifest: ' + str(output))


if __name__ == '__main__':
    main()
