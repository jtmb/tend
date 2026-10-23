#!/usr/bin/env python3
"""Remove Tend's integration while preserving the maintenance engine and history."""
import json, os, pwd, shutil, subprocess
from pathlib import Path
if os.geteuid()!=0:
    raise SystemExit('Run through pkexec or sudo.')
backup=Path('/var/lib/tend-install')
if not (backup/'user.json').exists():
    raise SystemExit('No Tend installation record found.')
record=json.loads((backup/'user.json').read_text())
for n in [60,30,15]:
    subprocess.run(['systemctl','disable','--now',f'tend-warning-{n}.timer'],check=False)
    Path(f'/etc/systemd/system/tend-warning-{n}.timer').unlink(missing_ok=True)
for p in ['/usr/share/polkit-1/actions/io.github.jtmb.Tend.policy','/etc/systemd/system/tend-warning@.service','/etc/systemd/system/dev-maintenance.timer.d/95-tend.conf','/etc/systemd/system/dev-maintenance.service.d/95-tend.conf','/etc/tend/integration-ready','/etc/tend/settings.json','/usr/local/bin/tend','/usr/local/libexec/tend-admin','/usr/local/share/applications/io.github.jtmb.Tend.desktop','/usr/local/share/icons/hicolor/scalable/apps/io.github.jtmb.Tend.svg']:
    Path(p).unlink(missing_ok=True)
engine=Path('/usr/local/lib/dev-maintenance/dev-maintenance.py')
if (backup/'engine.py').exists():
    # Preserve later engine edits: remove only Tend's known integration block.
    source=engine.read_text()
    start='    # Tend schedule integration: the calendar and due window use one setting.\n'
    end="    if (local.hour, local.minute) < (settings.get('hour', 8), settings.get('minute', 0)):\n"
    if start in source and end in source:
        a=source.index(start);b=source.index(end,a)+len(end)
        source=source[:a]+'    if local.hour < 8:\n'+source[b:]
        compile(source,str(engine),'exec');engine.write_text(source)
subprocess.run(['systemctl','daemon-reload'],check=True)
subprocess.run(['systemctl','enable' if record.get('maintenance_enabled',True) else 'disable','dev-maintenance.timer'],check=True)
subprocess.run(['systemctl','start' if record.get('maintenance_active',True) else 'stop','dev-maintenance.timer'],check=True)
uid=record['uid'];user=pwd.getpwuid(uid)
env=dict(os.environ,XDG_RUNTIME_DIR=f'/run/user/{uid}',DBUS_SESSION_BUS_ADDRESS=f'unix:path=/run/user/{uid}/bus')
for n,enabled in record['warnings'].items():
    if enabled:
        subprocess.run(['runuser','-u',user.pw_name,'--','systemctl','--user','enable','--now',f'dev-maintenance-warning-{n}.timer'],env=env,check=True)
print('Tend removed; the original maintenance schedule and reboot hook are active. Reports were preserved. Unpin Tend from the dock if desired.')
