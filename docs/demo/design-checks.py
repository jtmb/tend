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

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('mode', choices=['matrix', 'accessibility', 'panel', 'demo', 'gallery'])
args = parser.parse_args()
repo = Path(__file__).resolve().parents[2]
os.chdir(repo)
root = Path(subprocess.check_output(['python3','docs/demo/prepare.py'],text=True).strip())
env = dict(os.environ, GDK_BACKEND='x11', GSK_RENDERER='cairo', GTK_A11Y='none',
           GSETTINGS_BACKEND='memory', GIO_USE_VFS='local',
           XDG_CONFIG_HOME=str(root/'home/.config'), XDG_CACHE_HOME=str(root/'cache'),
           XDG_DATA_HOME=str(root/'data'), TEND_DEMO_HOME=str(root/'home'),
           TEND_DEMO_DIR=str(root/'frames'), DBUS_SESSION_BUS_ADDRESS=f'unix:path={root}/no-session-bus')
print(root, flush=True)
output = root/'checks'
output.mkdir()
records = []


def run(name, command, values=None, screen='3000x2000x24'):
    current = env | (values or {})
    with (output/f'{name}.log').open('w') as log:
        subprocess.run(['xvfb-run','-a','-s',f'-screen 0 {screen}',*command],env=current,
                       stdout=log,stderr=subprocess.STDOUT,check=True,timeout=240)
    print(name, 'passed', flush=True)


if args.mode == 'matrix':
    run('matrix',['target/debug/tend'],{'TEND_QA_DIR':str(root/'qa')})
    # Only policy JSON files are intentionally edited by the interaction checks.
    expected = json.loads((root/'manifest.json').read_text())
    for name, fingerprint in expected.items():
        if name.endswith('/cleanup.json') or name.endswith('/rules.json'):
            continue
        import hashlib
        p = root/name
        assert p.stat().st_size == fingerprint['bytes'] and hashlib.sha256(p.read_bytes()).hexdigest() == fingerprint['sha256'], name
elif args.mode == 'accessibility':
    for variant, settings in [('scale-200',{'GDK_SCALE':'2'}),('large-text',{'TEND_QA_TEXT_SCALE':'1.35'}),('high-contrast',{'ADW_DEBUG_HIGH_CONTRAST':'1'})]:
        for theme in [1,2]:
            (root/'home/.config/tend/theme').write_text(str(theme))
            for page in ['schedule','cleanup','daily-cleanup','cleanup-scheduling']:
                name = f'{variant}-{theme}-{page}'
                path = output/f'{name}.png'
                run(name,['target/debug/tend'],settings|{'TEND_SCREENSHOT':str(path),'TEND_SCREENSHOT_PAGE':page,'TEND_QA_WIDTH':'960' if variant == 'scale-200' else '720'})
                assert path.is_file()
                records.append({'variant':variant,'theme':theme,'page':page,'image':str(path)})
elif args.mode == 'panel':
    current = env.copy(); current.pop('DBUS_SESSION_BUS_ADDRESS')
    current['TEND_PANEL_QA'] = str(root/'panel-evidence.json')
    with (output/'panel.log').open('w') as log:
        subprocess.run(['dbus-run-session','--config-file=docs/demo/private-bus.conf','--',
                        'xvfb-run','-a','/usr/bin/python3','docs/demo/panel-check.py'],env=current,
                       stdout=log,stderr=subprocess.STDOUT,check=True,timeout=40)
    print((root/'panel-evidence.json').read_text(), flush=True)
elif args.mode == 'demo':
    run('demo',['target/debug/tend'])
elif args.mode == 'gallery':
    (root/'home/.config/tend/theme').write_text('2')
    for page, asset in [('overview','overview'),('task-manager','task-manager'),('cleanup','cleanup'),('system-logs','system-logs'),('tuning','performance')]:
        path = output/f'{asset}.png'
        run(asset,['target/debug/tend'],{'TEND_SCREENSHOT':str(path),'TEND_SCREENSHOT_PAGE':page,'TEND_QA_WIDTH':'1100','TEND_QA_HEIGHT':'720','TEND_TASK_TAB':'performance'})
        assert path.is_file()
        records.append({'page':page,'asset':asset,'image':str(path)})
if args.mode != 'matrix':
    subprocess.run(['python3','docs/demo/prepare.py','--verify',str(root)],check=True)
(output/'capture-index.json').write_text(json.dumps(records,indent=2)+'\n')
print('Evidence:',root,flush=True)
