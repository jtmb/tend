#!/usr/bin/env python3
"""Install Tend over an existing dev-maintenance deployment; invoked through pkexec."""
import json, os, pwd, re, shutil, subprocess
from pathlib import Path
if os.geteuid() != 0:
    raise SystemExit('Run scripts/install.sh as your desktop user.')
os.umask(0o022)
root = Path(__file__).resolve().parents[1]
uid = int(os.environ.get('PKEXEC_UID', os.environ.get('SUDO_UID', '-1')))
if uid <= 0:
    raise SystemExit('Install from the desktop user through pkexec or sudo.')
user = pwd.getpwuid(uid)
if not re.fullmatch(r'[a-zA-Z_][a-zA-Z0-9_-]*', user.pw_name):
    raise SystemExit('Unsupported local user name.')
engine = Path('/usr/local/lib/dev-maintenance/dev-maintenance.py')
if not engine.exists():
    raise SystemExit('Tend currently requires the existing dev-maintenance engine. See README prerequisites.')
source = engine.read_text()
needle = '    if local.hour < 8:\n'
if '# Tend schedule integration' not in source:
    if needle not in source:
        raise SystemExit('Unrecognized maintenance engine. No installation changes made.')
    source = source.replace(needle, '''    # Tend schedule integration: the calendar and due window use one setting.
    settings_path = Path('/etc/tend/settings.json')
    settings = json.loads(settings_path.read_text()) if settings_path.exists() else {}
    if (local.hour, local.minute) < (settings.get('hour', 8), settings.get('minute', 0)):
''', 1)
compile(source, str(engine), 'exec')
for binary in ['tend', 'tend-admin']:
    if not (root / 'target/release' / binary).is_file():
        raise SystemExit('Build the release binaries first.')
if subprocess.run(['systemctl','is-active','--quiet','dev-maintenance.service']).returncode == 0:
    raise SystemExit('Maintenance is running. Install after it finishes.')
backup = Path('/var/lib/tend-install')
backup.mkdir(mode=0o700, exist_ok=True)
if not (backup / 'engine.py').exists():
    shutil.copy2(engine, backup / 'engine.py')
manifest = backup / 'user.json'
user_env = dict(os.environ, XDG_RUNTIME_DIR=f'/run/user/{uid}', DBUS_SESSION_BUS_ADDRESS=f'unix:path=/run/user/{uid}/bus')
def userctl(*args):
    return subprocess.run(['runuser','-u',user.pw_name,'--','systemctl','--user',*args],env=user_env,capture_output=True)
if not manifest.exists():
    manifest.write_text(json.dumps({'uid':uid,'maintenance_enabled':subprocess.run(['systemctl','is-enabled','--quiet','dev-maintenance.timer']).returncode == 0,'maintenance_active':subprocess.run(['systemctl','is-active','--quiet','dev-maintenance.timer']).returncode == 0,'warnings': {str(n): userctl('is-enabled', f'dev-maintenance-warning-{n}.timer').returncode == 0 for n in [60,30,15]}}))
engine.write_text(source)
for name,target in [('tend','/usr/local/bin/tend'),('tend-admin','/usr/local/libexec/tend-admin')]:
    p=Path(target);p.parent.mkdir(parents=True,exist_ok=True)
    temporary=p.with_suffix('.new');shutil.copyfile(root/'target/release'/name, temporary);temporary.chmod(0o755);temporary.replace(p)
for name,target in [('io.github.jtmb.Tend.policy','/usr/share/polkit-1/actions/io.github.jtmb.Tend.policy'),('io.github.jtmb.Tend.svg','/usr/local/share/icons/hicolor/scalable/apps/io.github.jtmb.Tend.svg'),('io.github.jtmb.Tend.desktop','/usr/local/share/applications/io.github.jtmb.Tend.desktop')]:
    p=Path(target);p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(root/'assets'/name,p);p.chmod(0o644)
Path('/etc/tend').mkdir(mode=0o755,exist_ok=True)
for directory in ['/etc/systemd/system/dev-maintenance.timer.d','/etc/systemd/system/dev-maintenance.service.d']:
    p=Path(directory)
    if p.is_symlink() or (p.exists() and p.stat().st_uid != 0):
        raise SystemExit('Unprotected system destination: '+directory)
    p.mkdir(mode=0o755,exist_ok=True);p.chmod(0o755)
Path('/etc/tend/integration-ready').write_text('1\n')
Path('/etc/systemd/system/tend-warning@.service').write_text(f'''[Unit]
Description=Tend maintenance reminder
[Service]
Type=oneshot
User={user.pw_name}
Environment=XDG_RUNTIME_DIR=/run/user/{uid}
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{uid}/bus
ExecStart=/usr/local/libexec/tend-admin notify %i
''')
config=Path('/etc/tend/settings.json')
settings=config.read_text() if config.exists() else json.dumps(dict(enabled=True,hour=8,minute=0,reboot=True,warnings=[True,True,True]))
subprocess.run(['/usr/local/libexec/tend-admin','apply'],input=settings,text=True,check=True)
for n in [60,30,15]:
    userctl('disable','--now',f'dev-maintenance-warning-{n}.timer')
print('Tend installed. No maintenance run or reboot was started.')
