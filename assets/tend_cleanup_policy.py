"""Validated Tend daily cleanup integration; only fixed maintenance operations."""
import json
from pathlib import Path
import time

CONFIG = Path('/etc/tend/cleanup-policy.json')
DEFAULTS = dict(version=1, apt=True, docker_build=True, docker_images=True,
                npm_logs=True, npm_verify=True, build_days=14, image_days=14,
                keep_gb=5, npm_log_days=30, npm_verify_days=7)
PHASES = {'apt-cache': 'apt', 'docker-build': 'docker_build',
          'docker-images': 'docker_images', 'npm-logs': 'npm_logs',
          'npm-cache': 'npm_verify'}


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


def describe(policy):
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
    for phase, key in PHASES.items():
        lines.append(phase + ': ' + (commands[phase] if policy[key] else 'Skipped by policy'))
    return '\n'.join(lines)


def execute(name, state, policy, run_command, as_user, apt_locks_busy, npm_busy, now=None):
    if name not in PHASES:
        raise ValueError('Unknown cleanup phase')
    if isinstance(policy, Exception):
        return 'failed', 'Invalid saved daily policy: ' + str(policy)
    policy = validate(policy)
    if not policy[PHASES[name]]:
        return 'success', 'Skipped by policy'
    if name == 'apt-cache':
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
    return ('success', 'Completed') if not code else ('failed', 'Cleanup failed: ' + output[-2000:])
