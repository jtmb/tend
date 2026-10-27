"""Validated Tend daily cleanup integration; only fixed maintenance operations."""
import json
import contextlib
import fcntl
import os
import stat
from pathlib import Path
import time

CONFIG = Path('/etc/tend/cleanup-policy.json')
DEFAULTS = dict(version=1, apt=True, docker_build=True, docker_images=True,
                npm_logs=True, npm_verify=True, build_days=14, image_days=14,
                keep_gb=5, npm_log_days=30, npm_verify_days=7)
PHASES = {'apt-cache': 'apt', 'docker-build': 'docker_build',
          'docker-images': 'docker_images', 'npm-logs': 'npm_logs',
          'npm-cache': 'npm_verify'}


EXCLUSIONS = Path('/etc/tend/exclusions.json')
EXCLUSION_LOCK = Path('/etc/tend/exclusions.lock')
EXCLUSION_READY = Path('/etc/tend/exclusions-ready')
EXCLUSION_OWNER = 0


def validate_exclusions(raw):
    if not isinstance(raw, dict) or set(raw)-{'version','revision','paths'}:
        raise ValueError('Invalid exclusion object or unknown fields')
    result=dict(version=1,revision=0,paths=[]);result.update(raw)
    if type(result['version']) is not int or result['version']!=1 or type(result['revision']) is not int or not 0<=result['revision']<2**64 or not isinstance(result['paths'],list) or len(result['paths'])>64:
        raise ValueError('Invalid exclusion version, revision or count')
    seen=set()
    for entry in result['paths']:
        if not isinstance(entry,dict) or set(entry)!={'path','enabled','note'} or not isinstance(entry['path'],str) or type(entry['enabled']) is not bool or not isinstance(entry['note'],str):
            raise ValueError('Invalid protected-path fields')
        path=Path(entry['path'])
        if not path.is_absolute() or '..' in path.parts or len(entry['path'].encode())>4096 or any(ord(c)<32 or 127<=ord(c)<=159 for c in entry['path']+entry['note']) or len(entry['note'])>120 or path in seen:
            raise ValueError('Invalid or duplicate protected path')
        seen.add(path)
    return result


def _checked(fd):
    m=os.fstat(fd)
    if not stat.S_ISREG(m.st_mode) or m.st_uid!=EXCLUSION_OWNER or m.st_mode&0o022:
        raise ValueError('Exclusions need protected administrator-owned files')


def _parents(path):
    # Production directories must be administrator-owned all the way to /.
    # Tests override the owner and validate their private fixture directory.
    for parent in path.parents:
        m=parent.lstat()
        if not stat.S_ISDIR(m.st_mode) or stat.S_ISLNK(m.st_mode) or m.st_uid!=EXCLUSION_OWNER or m.st_mode&0o022:
            raise ValueError('Exclusions directory is not protected; cleanup stopped')
        if EXCLUSION_OWNER!=0:break


@contextlib.contextmanager
def exclusion_guard():
    fd=None
    try:
        try:
            if EXCLUSION_LOCK.exists() or EXCLUSION_LOCK.is_symlink():_parents(EXCLUSION_LOCK)
            fd=os.open(EXCLUSION_LOCK,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
            _checked(fd);fcntl.flock(fd,fcntl.LOCK_SH|fcntl.LOCK_NB)
        except FileNotFoundError:
            if EXCLUSIONS.exists() or EXCLUSIONS.is_symlink() or EXCLUSION_READY.exists():
                raise ValueError('Exclusion locking is missing; cleanup stopped')
        try:
            if EXCLUSIONS.exists() or EXCLUSIONS.is_symlink():_parents(EXCLUSIONS)
            policy_fd=os.open(EXCLUSIONS,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
        except FileNotFoundError:
            if fd is not None:
                raise ValueError('Exclusions policy is missing; cleanup stopped')
            yield validate_exclusions({});return
        with os.fdopen(policy_fd,'rb') as stream:
            _checked(stream.fileno());raw=stream.read(512*1024+1)
        if len(raw)>512*1024:
            raise ValueError('Exclusions exceed 512 KiB')
        yield validate_exclusions(json.loads(raw))
    finally:
        if fd is not None:os.close(fd)


def _resolved(path):
    missing=[];path=Path(path)
    while True:
        try:
            return path.resolve(strict=True).joinpath(*reversed(missing))
        except FileNotFoundError:
            if path==path.parent:raise ValueError('Protected path cannot be resolved')
            missing.append(path.name);path=path.parent


def apt_excluded(exclusions,run_command):
    active=[p['path'] for p in exclusions['paths'] if p['enabled']]
    if not active:return False
    code,value=run_command(['/usr/bin/apt-config','shell','TEND_ARCHIVES','Dir::Cache::archives/d'],timeout=15)
    import shlex
    words=shlex.split(value) if not code else []
    if len(words)!=1 or not words[0].startswith('TEND_ARCHIVES='):
        return True
    root=Path(words[0].split('=',1)[1])
    if not root.is_absolute():return True
    root=_resolved(root)
    for raw in active:
        for path in (Path(raw),_resolved(raw)):
            if root==path or root in path.parents or path in root.parents:return True
    return False


def validate(raw):
    if not isinstance(raw, dict) or set(raw) - set(DEFAULTS):
        raise ValueError('Unknown daily policy fields or invalid object')
    policy = {**DEFAULTS, **raw}
    for key, value in policy.items():
        if type(value) is not type(DEFAULTS[key]):
            raise ValueError('Invalid type for ' + key)
    if policy['version'] != 1:
        raise ValueError('Unsupported daily policy version')
    for key, low, high in [('build_days', 7, 3650), ('image_days', 7, 3650),
                           ('keep_gb', 1, 100), ('npm_log_days', 1, 3650),
                           ('npm_verify_days', 1, 30)]:
        if not low <= policy[key] <= high:
            raise ValueError('Out-of-range daily policy value: ' + key)
    return policy


def snapshot(path=CONFIG):
    """Invalid policy remains a failure value; never revert to enabled defaults."""
    try:
        try:
            raw = path.read_text()
        except FileNotFoundError:
            raw = '{}'
        return validate(json.loads(raw))
    except (OSError, ValueError) as exc:
        return exc


def describe(policy, run_command=None):
    with exclusion_guard() as exclusions:
        return _describe(policy, exclusions, run_command)


def _describe(policy, exclusions, run_command):
    if isinstance(policy, Exception):
        raise ValueError('Invalid saved daily policy: ' + str(policy))
    lines = []
    commands = {
        'apt-cache': 'apt-get autoclean',
        'docker-build': 'docker builder prune --all --force --filter until={}h --keep-storage {}GB'.format(policy['build_days'] * 24, policy['keep_gb']),
        'docker-images': 'docker image prune --force --filter until={}h'.format(policy['image_days'] * 24),
        'npm-logs': 'npm diagnostic logs older than {} days'.format(policy['npm_log_days']),
        'npm-cache': 'npm cache verify every {} days'.format(policy['npm_verify_days']),
    }
    active=any(p['enabled'] for p in exclusions['paths'])
    for phase, key in PHASES.items():
        command=commands[phase]
        if policy[key] and active and phase in ('docker-build','docker-images','npm-cache'):
            command='Skipped by exclusions: engine-managed cleanup cannot apply protected paths'
        elif policy[key] and active and phase=='apt-cache':
            if run_command is None:
                import subprocess
                def run_command(args,timeout):
                    result=subprocess.run(args,stdin=subprocess.DEVNULL,capture_output=True,text=True,timeout=timeout)
                    return result.returncode,result.stdout
            if apt_excluded(exclusions,run_command):command='Skipped by exclusions: protected or unverified APT cache location'
        elif policy[key] and active and phase=='npm-logs':
            command+='; protected paths omitted by the user worker'
        lines.append(phase + ': ' + (command if policy[key] else 'Skipped by policy'))
    return '\n'.join(lines)


def execute(name, state, policy, run_command, as_user, apt_locks_busy, npm_busy, now=None):
    if name not in PHASES:return 'failed','Unknown cleanup phase'
    if isinstance(policy,Exception):return 'failed','Invalid saved daily policy: '+str(policy)
    try:
        policy=validate(policy)
        if not policy[PHASES[name]]:return 'success','Skipped by policy'
        with exclusion_guard() as exclusions:
            return _execute(name,state,policy,run_command,as_user,apt_locks_busy,npm_busy,now,exclusions)
    except (OSError,ValueError,RuntimeError) as exc:
        return 'failed','Exclusion protection unavailable: '+str(exc)


def _execute(name, state, policy, run_command, as_user, apt_locks_busy, npm_busy, now, exclusions):
    if name not in PHASES:
        raise ValueError('Unknown cleanup phase')
    if isinstance(policy, Exception):
        return 'failed', 'Invalid saved daily policy: ' + str(policy)
    policy = validate(policy)
    if not policy[PHASES[name]]:
        return 'success', 'Skipped by policy'
    active=any(p['enabled'] for p in exclusions['paths'])
    if active and name in ('docker-build','docker-images','npm-cache'):
        return 'success','Skipped by exclusions: this engine-managed operation cannot apply protected paths'
    if name == 'apt-cache':
        if apt_excluded(exclusions,run_command):
            return 'success','Skipped by exclusions: protected or unverified APT cache location'
        if apt_locks_busy():
            return 'deferred', 'APT or dpkg is busy'
        code, output = run_command(['/usr/bin/apt-get', '-o', 'DPkg::Lock::Timeout=0', 'autoclean'], timeout=None)
    else:
        now = time.time() if now is None else now
        if name == 'npm-cache' and now - state.get('npm_last_verified', 0) < policy['npm_verify_days'] * 86400:
            return 'success', 'npm verification is not yet due'
        if name.startswith('npm-') and npm_busy():
            return 'deferred', 'Another npm operation is active'
        command = ['/usr/local/libexec/tend-worker', 'daily-phase', name,
                   json.dumps(policy, sort_keys=True)]
        code, output = run_command(as_user(command), timeout=1800)
        if code == 75 or (code and 'another cleanup' in output.lower()):
            return 'deferred', 'Another cleanup is active'
        if name == 'npm-cache' and not code:
            state['npm_last_verified'] = now
    return ('success', output[-2000:] or 'Completed') if not code else ('failed', 'Cleanup failed: ' + output[-2000:])


if __name__=='__main__':
    import sys
    if sys.argv[1:]==['--capabilities']:
        print(json.dumps(dict(protocol=3,role='maintenance-adapter',cleanup_exclusions=True,explained_cleanup=True,maintenance_events=False)))
    else:
        raise SystemExit('Only read-only --capabilities is supported directly')
