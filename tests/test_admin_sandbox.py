"""Run the real administrator helper in a retained, networkless mount sandbox.

Only fixture directories are writable. Every package/service/cleanup command is
replaced by a recorder. This tests helper wiring and rollback, not real updates.
Requires bubblewrap with unprivileged user namespaces and debug helper binaries.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / 'target/debug/tend-admin'
SETTINGS = dict(enabled=True, hour=8, minute=0, reboot=False, warnings=[True]*3)
MEMORY = dict(swappiness=65, zram_mb=4096, compression='lz4')
POLICY = dict(version=1, apt=True, docker_build=True, docker_images=True,
              npm_logs=True, npm_verify=True, build_days=14, image_days=14,
              keep_gb=5, npm_log_days=30, npm_verify_days=7)
MOCK = r'''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
name=Path(sys.argv[0]).name
args=sys.argv[1:]
entry={'program':name,'args':args}
if name=='runuser' and 'cleanup' in args:
    entry['input']=json.loads(sys.stdin.read())
if name=='unattended-upgrade':
    entry['config']=Path(os.environ['APT_CONFIG']).read_text()
    entry['needrestart']=os.environ.get('NEEDRESTART_MODE')
with Path('/qa/commands.jsonl').open('a') as f: f.write(json.dumps(entry)+'\n')
fail=os.environ.get('TEND_SANDBOX_FAIL','')
if fail==name or fail==name+':'+(args[0] if args else ''):
    print('Controlled failure: '+fail,file=sys.stderr);sys.exit(1)
if name=='id': print('1000' if len(args)>1 else os.environ.get('TEND_SANDBOX_UID','0'))
elif name=='getent': print('james:x:1000:1000:Fixture user:/home/james:/bin/bash')
elif name=='systemctl':
    if args[0]=='is-active': sys.exit(0 if args[-1]=='dev-maintenance.timer' else 3)
    if args[0]=='is-enabled': sys.exit(1)
elif name=='date': print('07:00:00')
elif name=='df': print('Avail\n80000000000')
elif name=='apt-config': print('Unattended-Upgrade::Automatic-Reboot "true";')
elif name=='crontab':
    path=Path('/qa/crontab')
    if args==['-l']:
        if path.exists(): print(path.read_text(),end='')
        else: print('no crontab for fixture',file=sys.stderr);sys.exit(1)
    elif args==['-']: path.write_text(sys.stdin.read())
elif name=='gsettings':
    path=Path('/qa/animations')
    if args[0]=='get': print(path.read_text().strip() if path.exists() else 'true')
    else: path.write_text(args[-1])
elif name=='runuser':
    if '/usr/bin/docker' in args:
        action=args[args.index('/usr/bin/docker')+1:]
        ident='a'*64
        if action[0]=='context': print('unix:///var/run/docker.sock')
        elif action[0]=='ps': print(ident)
        elif action[0]=='inspect': print(json.dumps(dict(Name='/sample',LogPath=f'/var/lib/docker/containers/{ident}/{ident}-json.log',LogConfig={'Type':'json-file','Config':{'max-size':'10m'}})))
    else: print('Mocked user cleanup completed; nothing deleted.')
else: print('Mocked '+name+' completed; no real command executed.')
'''


@unittest.skipUnless(shutil.which('bwrap') and BINARY.is_file(),
                     'Build debug tend-admin; bubblewrap is required')
class AdminSandboxTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        probe = subprocess.run(['bwrap','--unshare-user','--uid','0','--gid','0',
            '--ro-bind','/usr','/usr','--symlink','usr/lib','/lib',
            '--symlink','usr/lib64','/lib64','--','/usr/bin/true'],capture_output=True)
        if probe.returncode:
            raise unittest.SkipTest('Unprivileged mount namespaces unavailable')

    def setUp(self):
        Path('/tmp/opencode').mkdir(exist_ok=True)
        self.stage = Path(tempfile.mkdtemp(prefix='tend-admin-qa-',dir='/tmp/opencode'))
        for name in ['etc/tend','etc/systemd/system','etc/sysctl.d',
                     'etc/systemd/zram-generator.conf.d','var/lib/dev-maintenance',
                     'run','home/james','qa','power','generators']:
            (self.stage/name).mkdir(parents=True,exist_ok=True)
        self.write('etc/tend/settings.json',json.dumps(SETTINGS))
        self.write('etc/tend/integration-ready','1\n')
        self.write('etc/tend/cleanup-policy-ready','1\n')
        self.write('etc/tend/cleanup-policy.json',json.dumps(POLICY))
        self.write('qa/swappiness','60\n')
        self.write('generators/zram-generator','fixture generator; never executed\n')
        mock=self.write('qa/mock-command',MOCK);mock.chmod(0o755)
        (self.stage/'var/run').symlink_to('../run')
        ident='a'*64
        self.write(f'var/lib/docker/containers/{ident}/{ident}-json.log','sample log metadata\n')
        for p in [self.stage,*self.stage.rglob('*')]:
            if p.is_dir() and not p.is_symlink():
                p.chmod(0o755)
            elif p.is_file() and p.name!='mock-command':
                p.chmod(0o644)
        self.before = {p.relative_to(self.stage).as_posix():p.read_bytes()
                       for p in self.stage.rglob('*') if p.is_file()}

    def write(self,name,text):
        p=self.stage/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(text)
        return p

    def invoke(self,action,data=None,*extra,fail='',uid='0',api=False):
        cmd=['bwrap','--die-with-parent','--unshare-all','--new-session',
             '--uid','0','--gid','0','--cap-drop','ALL',
             '--ro-bind','/usr','/usr','--symlink','usr/lib','/lib',
             '--symlink','usr/lib64','/lib64','--proc','/proc','--dev','/dev',
             '--ro-bind','/sys','/sys']
        for folder in ['etc','var','run','home','qa']:
            cmd += ['--bind',str(self.stage/folder),'/'+folder]
        cmd += ['--bind',str(self.stage/'qa/swappiness'),'/proc/sys/vm/swappiness',
                '--ro-bind',str(self.stage/'power'),'/sys/class/power_supply',
                '--ro-bind',str(self.stage/'generators'),'/usr/lib/systemd/system-generators',
                '--ro-bind',str(ROOT/'target/debug/examples/qa-api' if api else BINARY),'/qa/tend-admin']
        for command in ['id','getent','systemctl','apt-get','apt-config',
                        'unattended-upgrade','date','notify-send','df','crontab','gsettings']:
            cmd += ['--ro-bind',str(self.stage/'qa/mock-command'),'/usr/bin/'+command]
        cmd += ['--ro-bind',str(self.stage/'qa/mock-command'),'/usr/sbin/runuser',
                '--clearenv','--setenv','PATH','/usr/bin:/usr/sbin',
                '--setenv','HOME','/home/james','--setenv','PKEXEC_UID','1000',
                '--setenv','TEND_QA_SANDBOX','1',
                '--setenv','TEND_SANDBOX_FAIL',fail,'--setenv','TEND_SANDBOX_UID',uid,
                '--','/qa/tend-admin',action,*extra]
        result=subprocess.run(cmd,input='' if data is None else json.dumps(data),
                              text=True,capture_output=True,timeout=30)
        with (self.stage/'qa/results.jsonl').open('a') as f:
            f.write(json.dumps(dict(action=action,code=result.returncode,
                                    stdout=result.stdout,stderr=result.stderr))+'\n')
        return result

    def commands(self):
        path=self.stage/'qa/commands.jsonl'
        return [json.loads(s) for s in path.read_text().splitlines()] if path.exists() else []

    def assert_ok(self,result):
        self.assertEqual(result.returncode,0,result.stderr)

    def tearDown(self):
        print('Retained administrator sandbox:',self.stage)

    def test_authorization_unknown_action_and_nonroot_rejection(self):
        self.assert_ok(self.invoke('authorization-check'))
        self.assertNotEqual(self.invoke('invalid').returncode,0)
        for action in ['apply','cleanup','daily-policy','updates','memory',
                       'memory-restore','install-zram','docker-logs','authorization-check']:
            with self.subTest(action=action):
                result=self.invoke(action,uid='1000')
                self.assertNotEqual(result.returncode,0)
                self.assertIn('authentication',result.stderr)
        self.assertTrue(all(c['program']=='id' for c in self.commands()))

    def test_schedule_save_disabled_and_command_failure_rolls_back(self):
        self.assert_ok(self.invoke('apply',SETTINGS))
        timer=(self.stage/'etc/systemd/system/dev-maintenance.timer.d/95-tend.conf').read_text()
        self.assertIn('08:00:00 America/Toronto',timer)
        service=(self.stage/'etc/systemd/system/dev-maintenance.service.d/95-tend.conf').read_text()
        self.assertEqual(service,'[Service]\nExecStartPost=\n')
        for lead,time in [(60,'07:00:00'),(30,'07:30:00'),(15,'07:45:00')]:
            self.assertIn(time,(self.stage/f'etc/systemd/system/tend-warning-{lead}.timer').read_text())
        baseline={str(p):p.read_bytes() for p in (self.stage/'etc').rglob('*') if p.is_file()}
        failed=self.invoke('apply',{**SETTINGS,'hour':9},fail='systemctl:start')
        self.assertNotEqual(failed.returncode,0)
        self.assertIn('restored previous files',failed.stderr)
        self.assertEqual(baseline,{str(p):p.read_bytes() for p in (self.stage/'etc').rglob('*') if p.is_file()})
        self.assert_ok(self.invoke('apply',{**SETTINGS,'enabled':False}))
        self.assertTrue(any(c['args'][0:2]==['disable','--now'] for c in self.commands() if c['program']=='systemctl'))

    def test_schedule_validation_and_warning_skips(self):
        failed=self.invoke('apply',{**SETTINGS,'hour':24})
        self.assertNotEqual(failed.returncode,0)
        self.assertIn('valid 24-hour',failed.stderr)
        self.assertFalse(any(c['program']=='systemctl' for c in self.commands()))
        self.assertNotEqual(self.invoke('notify',None,'12').returncode,0)
        self.write('etc/tend/settings.json',json.dumps({**SETTINGS,'enabled':False}))
        self.assert_ok(self.invoke('notify',None,'60'))
        self.assertFalse(any(c['program']=='notify-send' for c in self.commands()))

    def test_warning_dispatch(self):
        result=self.invoke('notify',None,'60')
        if result.returncode and not any(c['program']=='notify-send' for c in self.commands()):
            self.assertTrue(any(c['program']=='date' for c in self.commands()))
            self.skipTest('Host execution policy blocks mocked notify-send; notification delivery requires session verification')
        self.assert_ok(result)
        self.assertTrue(any(c['program']=='notify-send' for c in self.commands()))

    def test_daily_policy_validation_backup_and_missing_integration(self):
        changed={**POLICY,'build_days':60,'keep_gb':9}
        self.assert_ok(self.invoke('daily-policy',changed))
        self.assertEqual(json.loads((self.stage/'etc/tend/cleanup-policy.json').read_text()),changed)
        self.assertEqual(json.loads((self.stage/'etc/tend/cleanup-policy.previous.json').read_text()),POLICY)
        self.assertNotEqual(self.invoke('daily-policy',{**changed,'build_days':1}).returncode,0)
        # Retain the marker under a different name rather than deleting it.
        (self.stage/'etc/tend/cleanup-policy-ready').rename(self.stage/'etc/tend/cleanup-policy-ready.saved')
        self.assertNotEqual(self.invoke('daily-policy',changed).returncode,0)

    def test_cleanup_passes_exact_policy_and_reports_worker_failure(self):
        c=dict(apt=True,docker_build=False,docker_images=False,npm_logs=False,
               thumbnails=False,trash=False,pip=False,age_days=14,keep_gb=5)
        self.assert_ok(self.invoke('cleanup',c))
        commands=self.commands()
        self.assertTrue(any(x['program']=='apt-get' and x['args'][-1]=='autoclean' for x in commands))
        self.assertEqual(next(x['input'] for x in commands if 'input' in x)['age_days'],14)
        failed=self.invoke('cleanup',c,fail='runuser')
        self.assertNotEqual(failed.returncode,0)
        self.assertIn('User cleanup error',failed.stderr)

    def test_update_reboot_override_battery_and_failure(self):
        self.assert_ok(self.invoke('updates'))
        command=next(c for c in self.commands() if c['program']=='unattended-upgrade')
        self.assertTrue(command['config'].endswith('Unattended-Upgrade::Automatic-Reboot-WithUsers "false";\n'))
        self.assertIn('Unattended-Upgrade::Automatic-Reboot "false";',command['config'])
        self.assertEqual(command['needrestart'],'l')
        self.assertFalse((self.stage/'etc/tend/manual-updates.conf').exists())
        failed=self.invoke('updates',fail='unattended-upgrade')
        self.assertNotEqual(failed.returncode,0)
        self.assertFalse((self.stage/'etc/tend/manual-updates.conf').exists())
        self.write('power/BAT0/status','Discharging\n')
        failed=self.invoke('updates')
        self.assertNotEqual(failed.returncode,0)
        self.assertIn('Connect AC power',failed.stderr)

    def test_zram_prepare_memory_apply_validation_and_restore(self):
        self.assert_ok(self.invoke('install-zram'))
        self.assert_ok(self.invoke('memory',MEMORY))
        self.assertEqual((self.stage/'qa/swappiness').read_text(),'65')
        config=(self.stage/'etc/systemd/zram-generator.conf.d/99-tend.conf').read_text()
        self.assertIn('zram-size = min(ram / 2, 4096)',config)
        self.assertIn('compression-algorithm = lz4',config)
        self.assertNotEqual(self.invoke('memory',{**MEMORY,'swappiness':201}).returncode,0)
        self.assert_ok(self.invoke('memory-restore'))
        self.assertEqual((self.stage/'qa/swappiness').read_text(),'60')
        self.assertFalse((self.stage/'etc/tend/memory-backup.json').exists())
        self.assertNotEqual(self.invoke('memory-restore').returncode,0)

    def test_docker_log_metadata_and_lock_contention(self):
        self.assert_ok(self.invoke('docker-logs'))
        # Hold the same fixture lock inode from outside its namespace.
        import fcntl
        with (self.stage/'var/lib/dev-maintenance/run.lock').open('a') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            failed=self.invoke('updates')
            self.assertNotEqual(failed.returncode,0)
            self.assertIn('already running',failed.stderr)
        self.assertFalse(any(c['program']=='apt-get' for c in self.commands()))

    @unittest.skipUnless((ROOT/'target/debug/examples/qa-api').is_file(), 'Build docs-demo example qa-api')
    def test_real_cron_save_preserves_other_jobs_and_rolls_back_config(self):
        original='15 4 * * * echo unrelated-fixture\n'
        self.write('qa/crontab',original)
        self.write('home/james/scripts/tend-worker','fixture worker; never executed\n')
        schedule=dict(enabled=True,expression='30 9 * * 2',catch_up=True)
        self.assert_ok(self.invoke('schedule',None,json.dumps(schedule),api=True))
        config=self.stage/'home/james/.config/tend/cleanup-schedule.json'
        self.assertEqual(json.loads(config.read_text()),schedule)
        cron=(self.stage/'qa/crontab').read_text()
        self.assertTrue(cron.startswith(original))
        self.assertIn("30 9 * * 2 '/home/james/scripts/tend-worker' scheduled --apply",cron)
        self.assertIn('@reboot',cron)
        self.assertNotIn('reboot-after-maintenance',cron)
        failed=self.invoke('schedule',None,json.dumps({**schedule,'expression':'0 8 * * *'}),api=True,fail='crontab:-')
        self.assertNotEqual(failed.returncode,0)
        self.assertEqual(json.loads(config.read_text()),schedule)
        self.assertEqual((self.stage/'qa/crontab').read_text(),cron)
        self.assert_ok(self.invoke('schedule',None,json.dumps({**schedule,'enabled':False}),api=True))
        self.assertEqual((self.stage/'qa/crontab').read_text(),original)

    @unittest.skipUnless((ROOT/'target/debug/examples/qa-api').is_file(), 'Build docs-demo example qa-api')
    def test_real_startup_launcher_animation_and_recovery_contracts(self):
        source='[Desktop Entry]\nName=QA sample\nExec=sample %U\n'
        startup=self.write('home/james/.config/autostart/qa-sample.desktop',source)
        self.assert_ok(self.invoke('startup',None,'qa-sample.desktop','false',api=True))
        self.assertIn('Hidden=true',startup.read_text())
        self.assert_ok(self.invoke('restore',None,'0',api=True))
        self.assertEqual(startup.read_text(),source)
        launcher=self.write('home/james/.local/share/applications/code.desktop',source)
        self.assert_ok(self.invoke('wayland',None,'code.desktop',api=True))
        self.assertIn('--ozone-platform=wayland',launcher.read_text())
        self.assertNotEqual(self.invoke('wayland',None,'code.desktop',api=True).returncode,0)
        self.assert_ok(self.invoke('restore',None,'0',api=True))
        self.assertEqual(launcher.read_text(),source)
        self.assert_ok(self.invoke('animations',None,'false',api=True))
        self.assertEqual((self.stage/'qa/animations').read_text(),'false')
        self.assert_ok(self.invoke('restore',None,'0',api=True))
        self.assertEqual((self.stage/'qa/animations').read_text(),'true')
        self.assertNotEqual(self.invoke('restore',None,'0',api=True).returncode,0)
        self.write('home/james/.config/systemd/user/qa-sample.service','[Service]\nExecStart=/usr/bin/true\n')
        self.assert_ok(self.invoke('unit',None,'qa-sample.service','true',api=True))
        self.assert_ok(self.invoke('restore',None,'0',api=True))
        self.assertNotEqual(self.invoke('unit',None,'gnome-shell.service','false',api=True).returncode,0)
        service_commands=[x['args'] for x in self.commands() if x['program']=='systemctl']
        self.assertFalse(any('start' in x or 'stop' in x or 'restart' in x for x in service_commands))

    @unittest.skipUnless((ROOT/'target/debug/examples/qa-api').is_file(), 'Build docs-demo example qa-api')
    def test_process_signal_contract_on_new_private_namespace_children_only(self):
        for mode in ['quit','force','stale']:
            with self.subTest(mode=mode):
                self.assert_ok(self.invoke('signal',None,mode,api=True))


if __name__=='__main__':
    unittest.main()
