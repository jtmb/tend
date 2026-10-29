#!/usr/bin/env python3
"""Protect the shared desktop during a Cargo build. Dry-run by default.

Uses one transient user scope, never changes another app or installs a unit.
If memory tightens or a large Node job appears, interrupt only this build.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import uuid

MIB = 1024 * 1024
GIB = 1024 * MIB


def memory():
    values = {}
    for line in Path('/proc/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        if key in ('MemAvailable', 'SwapTotal', 'SwapFree'):
            values[key] = int(value.split()[0]) * 1024
    if len(values) != 3:
        raise ValueError('Required memory counters are unavailable')
    return values


def node_jobs():
    jobs = []
    for item in Path('/proc').iterdir():
        if not item.name.isdigit():
            continue
        try:
            if item.stat().st_uid != os.getuid():
                continue
            name = (item / 'comm').read_text().strip()
            if name not in ('node', 'nodejs'):
                continue
            fields = dict(line.split(':', 1) for line in (item / 'status').read_text().splitlines() if ':' in line)
            size = sum(int(fields.get(key, '0 kB').split()[0]) * 1024 for key in ('VmRSS', 'VmSwap'))
            jobs.append((int(item.name), size))
        except (FileNotFoundError, ProcessLookupError):
            continue
    return jobs


def reason(counters, jobs, starting):
    floor = 2 * GIB if starting else GIB
    swap_floor = 512 * MIB if starting else 256 * MIB
    if counters['MemAvailable'] < floor:
        return 'Available RAM is below the build reserve.'
    # Cold pages can remain swapped while RAM is available. Occupancy alone
    # must not defer every future build after a recovered memory incident.
    if (counters['SwapTotal'] and counters['SwapFree'] < swap_floor
            and counters['MemAvailable'] < floor + 512 * MIB):
        return 'Swap is nearly exhausted.'
    for pid, size in jobs:
        if size >= GIB:
            return f'Node PID {pid} already holds at least 1 GiB of resident/swapped mappings.'
    return None


def stop_owned_scope(unit, runner=subprocess.run):
    # Unit names come exclusively from the fresh random name below, never input.
    if not unit.startswith('tend-build-') or not unit.endswith('.scope'):
        raise ValueError('Refusing an unrelated scope')
    return runner(['/usr/bin/systemctl', '--user', '--no-ask-password', 'kill',
                   '--kill-whom=all', '--signal=SIGINT', unit],
                  stdin=subprocess.DEVNULL, capture_output=True, timeout=5)


def interrupt(child, unit):
    # Also cover the brief interval before systemd has created the scope.
    # Popen creates a fresh session/group containing only this invocation.
    if child.poll() is None:
        try:
            os.killpg(child.pid, signal.SIGINT)
        except ProcessLookupError:
            pass
    try:
        stop_owned_scope(unit)
    except (OSError, subprocess.SubprocessError):
        pass  # The private process group remains available if the bus fails.
    try:
        child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        if child.poll() is None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        try:
            subprocess.run(['/usr/bin/systemctl', '--user', '--no-ask-password', 'kill',
                            '--kill-whom=all', '--signal=SIGKILL', unit],
                           stdin=subprocess.DEVNULL, capture_output=True, timeout=5)
        except (OSError, subprocess.SubprocessError):
            pass
        child.wait(timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command and command[0] == '--':
        command = command[1:]
    if (len(command)<2 or Path(command[0]).name != 'cargo'
            or command[1] not in ('build','test','check','clippy','--version')):
        parser.error('Supply -- cargo build/test/check/clippy ...; application launch commands are not accepted.')
    if os.geteuid() == 0:
        parser.error('Run as the desktop user; no administrator authentication is needed.')
    counters = memory()
    deferred = reason(counters, node_jobs(), True)
    print(json.dumps({'mode': 'run' if args.run else 'dry-run', 'available_mib': counters['MemAvailable'] // MIB,
                      'memory_high_mib': 768, 'memory_max_mib': 1024, 'swap_max_mib': 256,
                      'deferred': deferred, 'persistent_units': False}), flush=True)
    if not args.run:
        return 0
    if deferred:
        print('Build deferred: ' + deferred, file=sys.stderr)
        return 75
    unit = 'tend-build-' + uuid.uuid4().hex + '.scope'
    launched = ['/usr/bin/systemd-run', '--user', '--scope', '--collect', '--quiet',
                '--no-ask-password', '--unit=' + unit, '--property=MemoryHigh=768M',
                '--property=MemoryMax=1G', '--property=MemorySwapMax=256M',
                '/usr/bin/nice', '-n', '10', '/usr/bin/ionice', '-c', '3', *command]
    env = dict(os.environ, CARGO_BUILD_JOBS='1')
    def terminated(number, _frame):
        raise SystemExit(128 + number)
    signal.signal(signal.SIGTERM, terminated)
    child = subprocess.Popen(launched, stdin=subprocess.DEVNULL, env=env, start_new_session=True)
    interrupted = False
    try:
        while child.poll() is None:
            try:
                blocked = reason(memory(), node_jobs(), False)
            except (OSError, ValueError) as error:
                blocked = 'Memory monitoring failed: ' + str(error)
            if blocked:
                interrupted = True
                print('Interrupting only this build: ' + blocked, file=sys.stderr, flush=True)
                interrupt(child, unit)
                break
            time.sleep(1)
    except BaseException:
        if child.poll() is None:
            interrupt(child, unit)
        raise
    return 75 if interrupted else child.returncode


if __name__ == '__main__':
    sys.exit(main())
