"""Run the real administrator helper in a retained, networkless mount sandbox.

Only fixture directories are writable. Every package/service/cleanup command is
replaced by a recorder. This tests helper wiring and rollback, not real updates.
Requires bubblewrap with unprivileged user namespaces and debug helper binaries.
"""
import json
import importlib.util
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
    entry['config']=Path(os.environ['APT_CONFIG']).read_text() if os.environ.get('APT_CONFIG') else None
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
elif name=='apt-config': print("TEND_ARCHIVES='/qa/apt-cache'" if args and args[0]=='shell' else 'Unattended-Upgrade::Automatic-Reboot "true";')
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
elif name=='docker':
    if args[0]=='context':
        path=Path('/qa/docker-endpoint')
        print(path.read_text() if path.exists() else 'unix:///var/run/docker.sock')
    elif args[0]=='ps': print('a'*64)
    elif args[0]=='inspect':
        delay=Path('/qa/docker-delay')
        if delay.exists():
            import time
            time.sleep(float(delay.read_text()))
        print(Path('/qa/docker-inspect.jsonl').read_text(),end='')
    else: raise RuntimeError('Unexpected Docker mutation or command')
elif name=='runuser':
    if 'capabilities' in args: print(json.dumps(dict(protocol=3,role='worker',cleanup_exclusions=True,explained_cleanup=True)))
    elif '/usr/bin/docker' in args:
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
        # Share immutable installed-mode binaries across retained fixtures. Each
        # sandbox mounts them read-only; avoid a new multi-MiB copy per test.
        Path('/tmp/opencode').mkdir(exist_ok=True)
        cls.binaries=Path(tempfile.mkdtemp(prefix='tend-qa-binaries-',dir='/tmp/opencode'))
        # An ELF verifier checks argv and controlled failure without writing
        # files or contacting a desktop bus from the notification subprocess.
        cls.notification=cls.binaries/'notify-send'
        source=r'''#include <stdio.h>
#include <stdlib.h>
#include <string.h>
int main(int argc,char **argv){
 if(argc!=5||strcmp(argv[1],"--app-name=Tend")||strcmp(argv[2],"--icon=io.github.jtmb.Tend")||strcmp(argv[3],"Maintenance in 60 minutes")||strcmp(argv[4],"Updates and cleanup begin at 08:00 Toronto time."))return 51;
 const char *fail=getenv("TEND_SANDBOX_FAIL");if(fail&&strcmp(fail,"notify-send")==0){fputs("Controlled notification failure\n",stderr);return 23;}return 0;
}
'''
        subprocess.run(['nice','-n','10','ionice','-c','3','cc','-x','c','-o',str(cls.notification),'-'],input=source,text=True,capture_output=True,check=True)
        cls.notification.chmod(0o755)
        # Inject directory-fsync failure only inside the private namespace;
        # ordinary file fsync still calls libc. Nothing is installed on the host.
        cls.directory_sync_failure=cls.binaries/'directory-sync-failure.so'
        sync_source=r'''#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <sys/stat.h>
#include <unistd.h>
int fsync(int fd){
 struct stat info;if(fstat(fd,&info)==0 && S_ISDIR(info.st_mode)){errno=EIO;return -1;}
 int(*original)(int)=(int(*)(int))dlsym(RTLD_NEXT,"fsync");
 if(!original){errno=ENOSYS;return -1;}return original(fd);
}
'''
        subprocess.run(['nice','-n','10','ionice','-c','3','cc','-shared','-fPIC','-x','c','-o',str(cls.directory_sync_failure),'-','-ldl'],input=sync_source,text=True,capture_output=True,check=True)
        cls.directory_sync_failure.chmod(0o755)
        for name in ('tend-admin','tend-worker'):
            target=cls.binaries/name
            shutil.copyfile(ROOT/'target/debug'/name,target);target.chmod(0o755)

    def setUp(self):
        Path('/tmp/opencode').mkdir(exist_ok=True)
        self.stage = Path(tempfile.mkdtemp(prefix='tend-admin-qa-',dir='/tmp/opencode'))
        for name in ['etc/tend','etc/systemd/system','etc/sysctl.d',
                     'etc/systemd/zram-generator.conf.d','var/lib/dev-maintenance',
                     'run','home/james','qa','power','generators',
                     'usr/local/libexec','usr/local/lib/dev-maintenance']:
            (self.stage/name).mkdir(parents=True,exist_ok=True)
        self.write('etc/tend/settings.json',json.dumps(SETTINGS))
        self.write('etc/tend/integration-ready','1\n')
        self.write('etc/tend/cleanup-policy-ready','1\n')
        self.write('etc/tend/cleanup-policy.json',json.dumps(POLICY))
        self.write('etc/tend/exclusions-ready','1\n')
        self.write('etc/tend/exclusions.lock','')
        self.write('etc/tend/exclusions.json',json.dumps(dict(version=1,revision=0,paths=[])))
        self.write('usr/local/lib/dev-maintenance/tend_cleanup_policy.py',(ROOT/'assets/tend_cleanup_policy.py').read_text())
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
        p.chmod(0o600 if name.startswith('home/james/.config/tend/') else 0o644)
        return p

    def invoke(self,action,data=None,*extra,fail='',uid='0',api=False,engine=False,fail_directory_sync=False):
        cmd=['bwrap','--die-with-parent','--unshare-all','--new-session',
             '--uid','0','--gid','0','--cap-drop','ALL',
             '--tmpfs','/usr',
             '--ro-bind','/usr/bin','/usr/bin','--ro-bind','/usr/sbin','/usr/sbin',
             '--ro-bind','/usr/lib','/usr/lib','--ro-bind','/usr/lib64','/usr/lib64',
             '--ro-bind','/usr/share','/usr/share','--symlink','usr/lib','/lib',
             '--symlink','usr/lib64','/lib64','--proc','/proc','--dev','/dev',
             '--ro-bind','/sys','/sys']
        for folder in ['etc','var','run','home','qa']:
            cmd += ['--bind',str(self.stage/folder),'/'+folder]
        cmd += ['--bind',str(self.stage/'usr/local'),'/usr/local']
        for name in ('tend-admin','tend-worker'):
            cmd+=['--ro-bind',str(self.binaries/name),'/usr/local/libexec/'+name]
        cmd += ['--bind',str(self.stage/'qa/swappiness'),'/proc/sys/vm/swappiness',
                '--ro-bind',str(self.stage/'power'),'/sys/class/power_supply',
                '--ro-bind',str(self.stage/'generators'),'/usr/lib/systemd/system-generators',
                '--ro-bind',str(ROOT/'target/debug/examples/qa-api' if api else BINARY),'/qa/tend-admin']
        for command in ['id','getent','systemctl','apt-get','apt-config',
                        'unattended-upgrade','date','notify-send','df','crontab','gsettings','docker']:
            cmd += ['--ro-bind',str(self.notification if command=='notify-send' else self.stage/'qa/mock-command'),'/usr/bin/'+command]
        entrypoint=['/qa/tend-admin']
        if engine:
            assert engine is True or engine=='qa-engine.py'
            entrypoint=['/usr/bin/python3','/usr/local/lib/dev-maintenance/'+('dev-maintenance.py' if engine is True else engine)]
        cmd += ['--ro-bind',str(self.stage/'qa/mock-command'),'/usr/sbin/runuser',
                '--ro-bind',str(self.stage/'qa/mock-command'),'/usr/sbin/on_ac_power',
                '--clearenv','--setenv','PATH','/usr/bin:/usr/sbin',
                '--setenv','HOME','/home/james','--setenv','PKEXEC_UID','1000',
                '--setenv','TEND_QA_SANDBOX','1',
                '--setenv','TEND_SANDBOX_FAIL',fail,'--setenv','TEND_SANDBOX_UID',uid,
                '--',*entrypoint,action,*extra]
        if fail_directory_sync:
            separator=cmd.index('--')
            cmd[separator:separator]=['--ro-bind',str(self.directory_sync_failure),'/qa/directory-sync-failure.so','--setenv','LD_PRELOAD','/qa/directory-sync-failure.so']
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

    def test_workspace_docker_queries_are_bounded_readonly_and_use_directory_labels(self):
        settings=dict(version=1,projects=[dict(id='project',name='Fixture',root='/projects/a')])
        container=dict(id='a'*64,name='/api',state='running',compose_directory='/projects/a',compose_project='same-name',compose_service='api')
        self.write('qa/docker-inspect.jsonl',json.dumps(container)+'\n')
        result=self.invoke('workspace-containers',None,json.dumps(settings),api=True)
        self.assert_ok(result)
        self.assertEqual(json.loads(result.stdout)[0]['workspace_id'],'project')
        commands=[c for c in self.commands() if c['program']=='docker']
        self.assertEqual([c['args'][0] for c in commands],['context','ps','inspect'])
        self.assertNotIn('.Config.Env',commands[-1]['args'][2])
        self.assertNotIn('.Config.Cmd',commands[-1]['args'][2])
        failed=self.invoke('workspace-containers',None,json.dumps(settings),api=True,fail='docker:inspect')
        self.assertNotEqual(failed.returncode,0)
        self.write('qa/docker-endpoint','ssh://fixture-remote')
        before=len(self.commands())
        remote=self.invoke('workspace-containers',None,json.dumps(settings),api=True)
        self.assertNotEqual(remote.returncode,0)
        self.assertIn('local Docker Unix socket',remote.stderr)
        self.assertEqual([c['args'][0] for c in self.commands()[before:]],['context'])

    def test_workspace_docker_timeout_and_output_limits_fail_without_mutations(self):
        settings=dict(version=1,projects=[])
        self.write('qa/docker-inspect.jsonl','x'*(2*1024*1024+20))
        large=self.invoke('workspace-containers',None,json.dumps(settings),api=True)
        self.assertNotEqual(large.returncode,0)
        self.assertIn('bounded query size',large.stderr)
        self.write('qa/docker-inspect.jsonl','{}\n')
        self.write('qa/docker-delay','8')
        delayed=self.invoke('workspace-containers',None,json.dumps(settings),api=True)
        self.assertNotEqual(delayed.returncode,0)
        self.assertIn('timed out',delayed.stderr)
        self.assertTrue(all(c['args'][0] in ('context','ps','inspect') for c in self.commands()))

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
        self.assert_ok(self.invoke('notify',None,'60'))
        failed=self.invoke('notify',None,'60',fail='notify-send')
        self.assertNotEqual(failed.returncode,0)
        self.assertIn('Controlled notification failure',failed.stderr)
        # The verifier returns 23 only after matching every actual argument.
        self.assertEqual(failed.stderr.strip(),'Controlled notification failure')
        self.write('etc/tend/settings.json',json.dumps({**SETTINGS,'enabled':False}))
        self.assert_ok(self.invoke('notify',None,'60',fail='notify-send'))

    def test_reviewed_user_settings_compare_under_lock_and_preserve_other_jobs(self):
        (self.stage/'home/james/cache').mkdir()
        self.write('qa/crontab','# existing job\n0 12 * * * echo keep\n')
        cases={'monitoring':{'hold_seconds':120},'history':{'max_mib':256,'retention_days':14},
               'workspaces':{'projects':[{'id':'fixture-project','name':'Fixture','root':'/home/james/project'}]},
               'manual_cleanup':{'age_days':28},
               'folder_rules':{'rules':[{'name':'Cache','folder':'/home/james/cache','pattern':'*.log','days':14,'recursive':False,'enabled':False}]},
               'user_schedule':{'enabled':False,'expression':'15 8 * * *','catch_up':False}}
        for section,changes in cases.items():
            with self.subTest(section=section):
                encoded=json.dumps(section)
                current=self.invoke('settings-current',None,encoded,api=True);self.assert_ok(current)
                expected=json.loads(current.stdout);value={**expected,**changes}
                saved=self.invoke('settings-checked',None,encoded,json.dumps(expected),json.dumps(value),api=True);self.assert_ok(saved)
                actual=self.invoke('settings-current',None,encoded,api=True);self.assert_ok(actual);self.assertEqual(json.loads(actual.stdout),value)
                commands=len(self.commands())
                stale=self.invoke('settings-checked',None,encoded,json.dumps(expected),json.dumps(expected),api=True)
                self.assertNotEqual(stale.returncode,0);self.assertIn('changed',stale.stderr.lower());self.assertEqual(len(self.commands()),commands)
                actual=self.invoke('settings-current',None,encoded,api=True);self.assertEqual(json.loads(actual.stdout),value)
        self.assertIn('0 12 * * * echo keep',(self.stage/'qa/crontab').read_text())
        self.assertNotIn('tend-worker',(self.stage/'qa/crontab').read_text())
        self.assertFalse((self.stage/'home/james/.local/state/tend/history.sqlite3').exists())
        self.assertFalse(any(c['program']=='systemctl' for c in self.commands()))

    def test_daily_policy_validation_backup_and_missing_integration(self):
        changed={**POLICY,'build_days':60,'keep_gb':9}
        self.assert_ok(self.invoke('daily-policy',changed))
        self.assertEqual(json.loads((self.stage/'etc/tend/cleanup-policy.json').read_text()),changed)
        self.assertEqual(json.loads((self.stage/'etc/tend/cleanup-policy.previous.json').read_text()),POLICY)
        self.assertNotEqual(self.invoke('daily-policy',{**changed,'build_days':1}).returncode,0)
        # Retain the marker under a different name rather than deleting it.
        (self.stage/'etc/tend/cleanup-policy-ready').rename(self.stage/'etc/tend/cleanup-policy-ready.saved')
        self.assertNotEqual(self.invoke('daily-policy',changed).returncode,0)

    def test_exclusions_save_revisions_backups_and_capability_refusal(self):
        value=dict(version=1,revision=0,paths=[dict(path='/home/james/keep',enabled=True,note='Keep')])
        self.assert_ok(self.invoke('exclusions-save',value))
        saved=json.loads((self.stage/'etc/tend/exclusions.json').read_text())
        self.assertEqual(saved,{**value,'revision':1})
        self.assertEqual(json.loads((self.stage/'etc/tend/exclusions.previous.json').read_text())['revision'],0)
        stale=self.invoke('exclusions-save',value)
        self.assertNotEqual(stale.returncode,0);self.assertIn('another session',stale.stderr)
        self.assertNotEqual(self.invoke('exclusions-save',{**saved,'restart':True}).returncode,0)
        self.assertNotEqual(self.invoke('exclusions-save',saved,fail='runuser').returncode,0)
        self.assertEqual(json.loads((self.stage/'etc/tend/exclusions.json').read_text()),saved)
        self.write('usr/local/lib/dev-maintenance/tend_cleanup_policy.py','print("{}")\n').chmod(0o644)
        old_adapter=self.invoke('exclusions-save',saved)
        self.assertNotEqual(old_adapter.returncode,0);self.assertIn('Matching exclusions integration',old_adapter.stderr)
        self.assertEqual(json.loads((self.stage/'etc/tend/exclusions.json').read_text()),saved)
        for path,data in self.before.items():
            if path.startswith('etc/') and '/exclusions' not in path:
                self.assertEqual((self.stage/path).read_bytes(),data)
        self.assertFalse(any(c['program'] in ('apt-get','systemctl') for c in self.commands()))

    def test_exclusions_cannot_change_during_cleanup_and_apt_obeys_protection(self):
        import fcntl
        value=dict(version=1,revision=0,paths=[])
        with (self.stage/'etc/tend/exclusions.lock').open() as lock:
            fcntl.flock(lock,fcntl.LOCK_SH|fcntl.LOCK_NB)
            busy=self.invoke('exclusions-save',value)
            self.assertNotEqual(busy.returncode,0);self.assertIn('active',busy.stderr)
        self.write('etc/tend/exclusions.json',json.dumps({**value,'paths':[dict(path='/qa/apt-cache/package',enabled=True,note='Keep')]})).chmod(0o644)
        self.assert_ok(self.invoke('apt-cleanup',dict(revision=0)))
        self.assertFalse(any(c['program']=='apt-get' for c in self.commands()))
        self.assertNotEqual(self.invoke('apt-cleanup',dict(revision=1)).returncode,0)
        self.write('etc/tend/exclusions.json',json.dumps(value)).chmod(0o644)
        self.assert_ok(self.invoke('apt-cleanup',dict(revision=0)))
        self.assertEqual(sum(c['program']=='apt-get' for c in self.commands()),1)

    def test_exclusion_editor_can_repair_a_path_that_became_unresolvable(self):
        (self.stage/'qa/loop').symlink_to('/qa/loop')
        empty=dict(version=1,revision=0,paths=[])
        broken={**empty,'paths':[dict(path='/qa/loop',enabled=True,note='Loop')]}
        failed=self.invoke('exclusions-save',broken)
        self.assertNotEqual(failed.returncode,0)
        self.assertEqual(json.loads((self.stage/'etc/tend/exclusions.json').read_text()),empty)
        self.write('etc/tend/exclusions.json',json.dumps(broken)).chmod(0o644)
        self.assert_ok(self.invoke('exclusions-save',empty))
        self.assertEqual(json.loads((self.stage/'etc/tend/exclusions.json').read_text()),{**empty,'revision':1})

    def test_maintenance_receipt_prevents_cleanup_replay_after_legacy_state_write_failure(self):
        import sqlite3
        spec=importlib.util.spec_from_file_location('history_integration_installer',ROOT/'scripts/install-policy.py')
        installer=importlib.util.module_from_spec(spec);spec.loader.exec_module(installer)
        source=installer.patch_engine((ROOT/'tests/fixtures/maintenance-engine.py.txt').read_text())
        self.write('usr/local/lib/dev-maintenance/dev-maintenance.py',source)
        self.write('usr/local/lib/dev-maintenance/tend_maintenance_history.py',(ROOT/'assets/tend_maintenance_history.py').read_text())
        self.write('etc/passwd','root:x:0:0:Fixture root:/root:/bin/bash\nfixture:x:1000:1000:Fixture:/home/fixture:/bin/bash\n')
        launcher='''import importlib.util,sys
from pathlib import Path
spec=importlib.util.spec_from_file_location('fixture_engine',Path(__file__).with_name('dev-maintenance.py'))
engine=importlib.util.module_from_spec(spec);spec.loader.exec_module(engine)
original=engine.atomic_json
def fail_state_once(path,value,mode):
    flag=Path('/qa/fail-state-once')
    if path.name=='state.json' and value.get('phases',{}).get('apt-cache',{}).get('success') and flag.exists():
        flag.rename('/qa/retained-fault-marker')
        raise OSError('Controlled fixture state-write failure after cleanup')
    return original(path,value,mode)
engine.atomic_json=fail_state_once
sys.exit(engine.main())'''
        self.write('usr/local/lib/dev-maintenance/qa-engine.py',launcher)
        for path in (self.stage/'usr/local/lib/dev-maintenance').iterdir():path.chmod(0o644)
        (self.stage/'etc/passwd').chmod(0o644)
        self.write('qa/fail-state-once','fail once')
        first=self.invoke('--run',engine='qa-engine.py')
        self.assertNotEqual(first.returncode,0);self.assertIn('Controlled fixture state-write failure',first.stderr)
        path=self.stage/'var/lib/dev-maintenance/state.json';state=json.loads(path.read_text())
        self.assertEqual(state['phases']['apt-cache']['status'],'running')
        db_path=self.stage/'var/lib/dev-maintenance/tend-history.sqlite3'
        with sqlite3.connect('file:'+str(db_path)+'?mode=ro',uri=True) as db:
            row=db.execute("SELECT status,payload FROM receipts WHERE phase='apt-cache'").fetchone()
            self.assertEqual(row[0],'success');completed=json.loads(row[1])
        state['phases']['apt-cache']['last_attempt']=0
        path.with_suffix('.retained-started.json').write_bytes(path.read_bytes())
        path.write_text(json.dumps(state));path.chmod(0o600)
        second=self.invoke('--run',engine=True)
        self.assert_ok(second)
        after=json.loads(path.read_text())
        self.assertEqual(after['phases']['apt-cache']['last_success'],completed['finished_ms']/1000)
        self.assertIn('action not repeated',after['phases']['apt-cache']['message'])
        autoclean=[c for c in self.commands() if c['program']=='apt-get' and c['args'][-1]=='autoclean']
        self.assertEqual(len(autoclean),1)
        with sqlite3.connect('file:'+str(db_path)+'?mode=ro',uri=True) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM events WHERE phase='apt-cache'").fetchone()[0],2)
            self.assertEqual(db.execute('SELECT count(DISTINCT phase) FROM events').fetchone()[0],7)
        public=self.invoke('maintenance-history',uid='1000')
        self.assert_ok(public);records=json.loads(public.stdout)
        self.assertEqual(len(records['events']),7)
        self.assertTrue(any(e['title']=='APT download cache' and e['outcome']=='Success' for e in records['events']))
        self.assertFalse(any(c['program']=='systemctl' for c in self.commands()))

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

    def test_typed_cleanup_rechecks_exact_files_and_global_policy_in_private_namespace(self):
        import time
        cleanup=dict(apt=False,docker_build=False,docker_images=False,npm_logs=True,
                     thumbnails=False,trash=False,pip=False,age_days=14,keep_gb=5,npm_log_days=30)
        for name in ('protected.log','eligible.log','replace.log'):
            path=self.write('home/james/.npm/_logs/'+name,'retained fixture '+name)
            os.utime(path,(time.time()-60*86400,)*2)
        excluded=dict(version=1,revision=0,paths=[dict(path='/home/james/.npm/_logs/protected.log',enabled=True,note='Keep')])
        self.write('etc/tend/exclusions.json',json.dumps(excluded)).chmod(0o644)
        preview=self.invoke('cleanup-plan',None,json.dumps(cleanup),'preview',uid='1000',api=True)
        self.assert_ok(preview);self.assertIn('2 files',preview.stdout);self.assertIn('1 protected',preview.stdout)
        self.assertIn('Why:',preview.stdout);self.assertIn('Effect:',preview.stdout)
        changed=self.invoke('cleanup-plan',None,json.dumps(cleanup),'policy-change',uid='1000',api=True)
        self.assertNotEqual(changed.returncode,0);self.assertIn('Exclusions changed',changed.stderr)
        self.assertTrue((self.stage/'home/james/.npm/_logs/eligible.log').exists())
        applied=self.invoke('cleanup-plan',None,json.dumps(cleanup),'replace',uid='1000',api=True)
        self.assert_ok(applied);self.assertIn('removed 1 files; 1 changed or protected',applied.stdout)
        self.assertFalse((self.stage/'home/james/.npm/_logs/eligible.log').exists())
        self.assertEqual((self.stage/'home/james/.npm/_logs/replace.log').read_bytes(),b'replacement must remain')
        self.assertTrue((self.stage/'home/james/.npm/_logs/replace.retained-original').exists())
        self.assertTrue((self.stage/'home/james/.npm/_logs/protected.log').exists())

    def test_pip_cache_alias_cannot_bypass_a_protected_target(self):
        target=self.stage/'home/james/keep-pip';target.mkdir()
        cache=self.stage/'home/james/.cache';cache.mkdir()
        (cache/'pip').symlink_to('/home/james/keep-pip',target_is_directory=True)
        value=dict(version=1,revision=0,paths=[dict(path='/home/james/keep-pip',enabled=True,note='Keep')])
        self.write('etc/tend/exclusions.json',json.dumps(value)).chmod(0o644)
        cleanup=dict(apt=False,docker_build=False,docker_images=False,npm_logs=False,pip=True)
        preview=self.invoke('cleanup-plan',None,json.dumps(cleanup),'preview',uid='1000',api=True)
        self.assert_ok(preview);self.assertIn('protected path overlaps the pip cache',preview.stdout)
        applied=self.invoke('cleanup-plan',None,json.dumps(cleanup),'apply',uid='1000',api=True)
        self.assert_ok(applied);self.assertIn('skipped',applied.stdout)
        self.assertTrue(target.exists())
        self.assertTrue(all(c['program']=='id' for c in self.commands()))

    def test_user_schedule_receipt_survives_audit_failure_and_blocks_uncertain_replay(self):
        config='home/james/.config/tend/'
        self.write(config+'cleanup-schedule.json',json.dumps(dict(enabled=True,expression='0 0 1 1 *',catch_up=True)))
        self.write(config+'cleanup.json',json.dumps(dict(apt=False,docker_build=False,docker_images=False,npm_logs=False,pip=False,thumbnails=False,trash=False)))
        self.write(config+'rules.json','{"rules":[]}')
        history=self.write('home/james/.local/state/tend/history.sqlite3','preserved corrupt fixture')
        history.chmod(0o600);history.parent.chmod(0o700)
        first=self.invoke('scheduled-run',api=True,uid='1000')
        self.assert_ok(first);self.assertIn('history could not be saved',first.stdout)
        path=history.with_name('scheduled-cleanup.json');saved=json.loads(path.read_text())
        self.assertIn('last_due',saved);self.assertNotIn('running_due',saved)
        before=path.read_bytes();second=self.invoke('scheduled-run',api=True,uid='1000')
        self.assert_ok(second);self.assertIn('already completed',second.stdout);self.assertEqual(path.read_bytes(),before)
        path.write_text(json.dumps(dict(running_due=saved['last_due'],attempts=0)));path.chmod(0o600)
        count=len(self.commands());uncertain=self.invoke('scheduled-run',api=True,uid='1000')
        self.assertNotEqual(uncertain.returncode,0);self.assertIn('uncertain',uncertain.stderr);self.assertEqual(len(self.commands()),count)
        path.write_text('{broken')
        malformed=self.invoke('scheduled-run',api=True,uid='1000')
        self.assertNotEqual(malformed.returncode,0);self.assertIn('Invalid cleanup receipt',malformed.stderr);self.assertEqual(path.read_text(),'{broken')
        for field in ['last_due','failed_due','finished_unix','running_due','started_unix','previous_uncertain_due']:
            path.write_text(json.dumps({field:2**64-1}));before=path.read_bytes()
            overflow=self.invoke('scheduled-run',api=True,uid='1000')
            self.assertNotEqual(overflow.returncode,0);self.assertIn('Invalid cleanup receipt',overflow.stderr);self.assertEqual(path.read_bytes(),before)
            self.assertEqual(len(self.commands()),count)
        path.write_text('{}');history.parent.chmod(0o500)
        mutations=sum(c['program']=='id' for c in self.commands())
        stopped=self.invoke('scheduled-run',api=True,uid='1000')
        self.assertNotEqual(stopped.returncode,0);self.assertEqual(path.read_text(),'{}')
        self.assertEqual(sum(c['program']=='id' for c in self.commands()),mutations)
        history.parent.chmod(0o700)
        self.assertEqual(history.read_text(),'preserved corrupt fixture')

    def test_user_schedule_stops_before_cleanup_if_receipt_directory_cannot_be_synced(self):
        config='home/james/.config/tend/'
        self.write(config+'cleanup-schedule.json',json.dumps(dict(enabled=True,expression='0 0 1 1 *',catch_up=True)))
        self.write(config+'cleanup.json',json.dumps(dict(apt=False,docker_build=False,docker_images=False,npm_logs=False,pip=False,thumbnails=False,trash=False)))
        self.write(config+'rules.json','{"rules":[]}')
        result=self.invoke('scheduled-run',api=True,uid='1000',fail_directory_sync=True)
        self.assertNotEqual(result.returncode,0);self.assertIn('receipt directory sync failed',result.stderr)
        receipt=self.stage/'home/james/.local/state/tend/scheduled-cleanup.json';self.assertIn('running_due',json.loads(receipt.read_text()))
        self.assertFalse(any(command['program']=='id' for command in self.commands()),'Cleanup execution began before receipt durability was established')

    def test_user_schedule_receipt_supports_an_existing_state_parent_alias(self):
        config='home/james/.config/tend/'
        self.write(config+'cleanup-schedule.json',json.dumps(dict(enabled=True,expression='0 0 1 1 *',catch_up=True)))
        self.write(config+'cleanup.json',json.dumps(dict(apt=False,docker_build=False,docker_images=False,npm_logs=False,pip=False,thumbnails=False,trash=False)))
        self.write(config+'rules.json','{"rules":[]}')
        home=self.stage/'home/james';target=home/'receipt-state';target.mkdir()
        (home/'.local').symlink_to('/home/james/receipt-state',target_is_directory=True)
        result=self.invoke('scheduled-run',api=True,uid='1000');self.assert_ok(result)
        receipt=json.loads((target/'state/tend/scheduled-cleanup.json').read_text());self.assertIn('last_due',receipt);self.assertNotIn('running_due',receipt)

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
