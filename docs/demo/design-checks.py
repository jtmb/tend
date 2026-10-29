#!/usr/bin/env python3
"""Run isolated native design checks; no installed app or system writes.

Requires an existing docs-demo binary, xvfb-run, optional dbus-run-session and
system Python GIO for the panel check. Every invocation retains a fresh fixture.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import resource
import signal

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('mode', choices=['matrix', 'accessibility', 'panel', 'demo', 'gallery', 'expansion', 'cleanup', 'activity', 'health', 'bundles', 'release', 'rules', 'memory'])
parser.add_argument('--parent',type=Path,default=Path('/tmp/opencode'),help='Retained fixture directory; history tests require at least 2 GiB free')
parser.add_argument('--binary',type=Path,default=Path('target/debug/tend'),help='Previously built docs-demo binary; never the installed application')
parser.add_argument('--variant',choices=['standard','scale-200','large-text','high-contrast'],default='standard')
parser.add_argument('--theme',choices=['light','dark'])
args = parser.parse_args()
repo = Path(__file__).resolve().parents[2]
os.chdir(repo)
binary=str(args.binary.expanduser().resolve())
assert Path(binary).is_file(),binary
root = Path(subprocess.check_output(['python3','docs/demo/prepare.py','--parent',str(args.parent.expanduser().resolve())],text=True).strip())
env = dict(os.environ, GDK_BACKEND='x11', GSK_RENDERER='cairo', GTK_A11Y='none',
           GSETTINGS_BACKEND='memory', GIO_USE_VFS='local',
           XDG_CONFIG_HOME=str(root/'home/.config'), XDG_CACHE_HOME=str(root/'cache'),
           XDG_DATA_HOME=str(root/'data'), TEND_DEMO_HOME=str(root/'home'),
           TEND_DEMO_DIR=str(root/'frames'), DBUS_SESSION_BUS_ADDRESS=f'unix:path={root}/no-session-bus')
variants={'standard':{},'scale-200':{'GDK_SCALE':'2'},'large-text':{'TEND_QA_TEXT_SCALE':'1.35'},'high-contrast':{'ADW_DEBUG_HIGH_CONTRAST':'1'}}
env.update(variants[args.variant]);env['TEND_QA_BINARY']=binary
if args.theme:
    theme_id='1' if args.theme=='light' else '2'
    (root/'home/.config/tend/theme').write_text(theme_id)
    env['TEND_QA_THEME_ID']=theme_id
print(root, flush=True)
output = root/'checks'
output.mkdir()
records = []
initial_pressure=Path('/proc/pressure/cpu').read_text()


def run(name, command, values=None, screen='3000x2000x24'):
    current = env | (values or {})
    if current.get('GDK_SCALE')=='2':screen='3000x2000x24'
    with (output/f'{name}.log').open('w') as log:
        command = ['xvfb-run','-a','-s',f'-screen 0 {screen}',*command]
        process = subprocess.Popen(command,env=current,stdout=log,stderr=subprocess.STDOUT,
                                   start_new_session=True,
                                   preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_CORE,(0,0)))
        try:
            code = process.wait(timeout=240)
            if code:
                raise subprocess.CalledProcessError(code,command)
        except BaseException:
            # Own process group only: failed capture children must not keep
            # consuming CPU after their Xvfb wrapper exits or times out.
            try: os.killpg(process.pid,signal.SIGTERM)
            except ProcessLookupError: pass
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                try: os.killpg(process.pid,signal.SIGKILL)
                except ProcessLookupError: pass
                process.wait()
            raise
    print(name, 'passed', flush=True)


if args.mode == 'matrix':
    run('matrix',[binary],{'TEND_QA_DIR':str(root/'qa')})
    # Only policy JSON files are intentionally edited by the interaction checks.
    expected = json.loads((root/'manifest.json').read_text())
    for name, fingerprint in expected.items():
        if name.endswith('/cleanup.json') or name.endswith('/rules.json'):
            continue
        import hashlib
        p = root/name
        assert p.stat().st_size == fingerprint['bytes'] and hashlib.sha256(p.read_bytes()).hexdigest() == fingerprint['sha256'], name
elif args.mode == 'memory':
    # Recorded kernel counters, anonymized boot/scope; never create pressure.
    (root/'home/demo-oom.jsonl').write_bytes((repo/'tests/fixtures/opencode-kernel-oom.jsonl').read_bytes())
    run('memory',[binary],{'TEND_MEMORY_QA':str(root/'memory')},screen='1600x1100x24')
    print((root/'memory/memory-evidence.json').read_text(),flush=True)
elif args.mode == 'rules':
    run('rules',[binary],{'TEND_RULES_QA':str(root/'rules')},screen='1600x1000x24')
    print((root/'rules/rule-editor-evidence.json').read_text(),flush=True)
elif args.mode == 'release':
    run('release',[binary],{'TEND_RELEASE_QA':str(root/'release')})
    print((root/'release/release-evidence.json').read_text(),flush=True)
elif args.mode == 'expansion':
    run('expansion',[binary],{'TEND_EXPANSION_QA':str(root/'expansion')},screen='1600x1000x24')
    print((root/'expansion/expansion-evidence.json').read_text(),flush=True)
elif args.mode == 'cleanup':
    run('cleanup',[binary],{'TEND_CLEANUP_QA':str(root/'cleanup')},screen='1600x1000x24')
    print((root/'cleanup/cleanup-evidence.json').read_text(),flush=True)
elif args.mode == 'activity':
    import datetime as dt
    import importlib.util
    from zoneinfo import ZoneInfo
    spec=importlib.util.spec_from_file_location('demo_maintenance_history',repo/'assets/tend_maintenance_history.py')
    history=importlib.util.module_from_spec(spec);spec.loader.exec_module(history)
    state=root/'home/.local/state/tend';state.mkdir(parents=True,exist_ok=True);state.chmod(0o700)
    local=dt.datetime.now(ZoneInfo('America/Toronto'))
    last=local.date() if (local.hour,local.minute)>=(8,15) else local.date()-dt.timedelta(days=1)
    clock={'wall':0,'boot':1000000,'free':8_000_000_000}
    with history.PhaseHistory(state,owner=os.geteuid(),wall_ms=lambda:clock['wall'],boot_ms=lambda:clock['boot'],boot_id=lambda:'fixture-boot',free=lambda:clock['free']) as writer:
        for i in range(25):
            day=last-dt.timedelta(days=24-i)
            base=int(dt.datetime.combine(day,dt.time(8),ZoneInfo('America/Toronto')).timestamp()*1000)
            for n,phase in enumerate(history.PHASES):
                for attempt in (1,2):
                    clock['wall']=base+n*60000+attempt*2000;clock['boot']+=5000
                    ticket,previous=writer.begin(phase,day.isoformat(),attempt,scheduled=base)
                    assert previous is None
                    if i==24 and phase=='health' and attempt==2:continue
                    clock['wall']+=1000;clock['boot']+=1000;clock['free']+=1024
                    writer.finish(ticket,'success' if attempt==2 else 'failed',f'Fixture {phase} outcome; no operation was executed')
    (state/history.FILENAME).rename(state/'demo-maintenance.sqlite3')
    run('activity',[binary],{'TEND_ACTIVITY_QA':str(root/'activity')},screen='1600x1000x24')
    print((root/'activity/activity-evidence.json').read_text(),flush=True)
elif args.mode == 'accessibility':
    for variant, settings in [('scale-200',{'GDK_SCALE':'2'}),('large-text',{'TEND_QA_TEXT_SCALE':'1.35'}),('high-contrast',{'ADW_DEBUG_HIGH_CONTRAST':'1'})]:
        for theme in [1,2]:
            (root/'home/.config/tend/theme').write_text(str(theme))
            for page in ['overview','cleanup','daily-cleanup','cleanup-scheduling','storage','task-manager','diagnostics','workspaces','system-logs','startup','tuning','development','reports','recovery','care','schedule','activity']:
                name = f'{variant}-{theme}-{page}'
                path = output/f'{name}.png'
                run(name,[binary],settings|{'TEND_SCREENSHOT':str(path),'TEND_SCREENSHOT_PAGE':page,'TEND_QA_WIDTH':'960' if variant == 'scale-200' else '720'})
                assert path.is_file()
                records.append({'variant':variant,'theme':theme,'page':page,'image':str(path)})
elif args.mode == 'bundles':
    run('bundles',[binary],{'TEND_BUNDLE_QA':str(root/'bundles')},screen='1600x1000x24')
    print((root/'bundles/bundle-evidence.json').read_text(),flush=True)
    import hashlib
    expected=json.loads((root/'manifest.json').read_text())
    for name,fingerprint in expected.items():
        if name.endswith('/cleanup.json') or name.endswith('/rules.json'):continue
        p=root/name
        assert p.stat().st_size==fingerprint['bytes'] and p.stat().st_mtime_ns==fingerprint['mtime_ns'] and hashlib.sha256(p.read_bytes()).hexdigest()==fingerprint['sha256'],name
    print('All four cache/journal inputs preserved exactly; only intended fixture settings changed',flush=True)
elif args.mode == 'health':
    current=env.copy();current.pop('DBUS_SESSION_BUS_ADDRESS')
    current['TEND_HEALTH_QA']=str(root/'health')
    with (output/'health.log').open('w') as log:
        subprocess.run(['dbus-run-session','--config-file=docs/demo/private-bus.conf','--','xvfb-run','-a','-s','-screen 0 3000x2000x24','/usr/bin/python3','docs/demo/health-check.py'],env=current,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=90)
    print((root/'health/health-evidence.json').read_text(),flush=True)
elif args.mode == 'panel':
    current = env.copy(); current.pop('DBUS_SESSION_BUS_ADDRESS')
    current['TEND_PANEL_QA'] = str(root/'panel-evidence.json')
    with (output/'panel.log').open('w') as log:
        subprocess.run(['dbus-run-session','--config-file=docs/demo/private-bus.conf','--',
                        'xvfb-run','-a','/usr/bin/python3','docs/demo/panel-check.py'],env=current,
                       stdout=log,stderr=subprocess.STDOUT,check=True,timeout=40)
    print((root/'panel-evidence.json').read_text(), flush=True)
elif args.mode == 'demo':
    run('demo',[binary])
elif args.mode == 'gallery':
    (root/'home/.config/tend/theme').write_text('2')
    for page, asset in [('overview','overview'),('task-manager','task-manager'),('cleanup','cleanup'),('system-logs','system-logs'),('tuning','performance')]:
        path = output/f'{asset}.png'
        run(asset,[binary],{'TEND_SCREENSHOT':str(path),'TEND_SCREENSHOT_PAGE':page,'TEND_QA_WIDTH':'1100','TEND_QA_HEIGHT':'760','TEND_TASK_TAB':'performance'})
        assert path.is_file()
        records.append({'page':page,'asset':asset,'image':str(path)})
if args.mode not in ('matrix','bundles'):
    subprocess.run(['python3','docs/demo/prepare.py','--verify',str(root)],check=True)
(output/'run-context.json').write_text(json.dumps({'binary':binary,'variant':args.variant,'theme':args.theme,'mode':args.mode,'nice':os.getpriority(os.PRIO_PROCESS,0),'cpu_pressure_before':initial_pressure,'cpu_pressure_after':Path('/proc/pressure/cpu').read_text()},indent=2)+'\n')
(output/'capture-index.json').write_text(json.dumps(records,indent=2)+'\n')
print('Evidence:',root,flush=True)

if args.mode == 'release' and not json.loads((root/'release/release-evidence.json').read_text())['interaction_target_met']:
    raise SystemExit('Native workflows completed, but the interaction latency target was not met; retained evidence contains the timings.')
if args.mode == 'rules' and not json.loads((root/'rules/rule-editor-evidence.json').read_text())['passed']:
    raise SystemExit('Native rule editor overwrote concurrent changes or could not reload; retained evidence contains the observed results.')
