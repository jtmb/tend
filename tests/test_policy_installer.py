"""Exercise installation/rollback in a retained unprivileged staging tree."""
from pathlib import Path
import hashlib
import importlib.util
import json
import os
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]

@unittest.skipUnless(all((ROOT/'target/release'/n).is_file() for n in ('tend-admin','tend-worker')),'Build release helpers for staging validation')
class InstallerTests(unittest.TestCase):
    def test_worker_scheduled_dry_run_includes_builtin_and_custom_policy(self):
        prepared=subprocess.run(['python3',str(ROOT/'docs/demo/prepare.py')],text=True,capture_output=True,check=True)
        stage=Path(prepared.stdout.strip());home=stage/'home';config=home/'.config/tend'
        policy=json.loads((config/'cleanup.json').read_text());policy['npm_logs']=True
        (config/'cleanup.json').write_text(json.dumps(policy))
        (config/'cleanup-schedule.json').write_text(json.dumps(dict(enabled=True,expression='* * * * *',catch_up=True)))
        paths=[p for p in home.rglob('*') if p.is_file()]
        before={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
        result=subprocess.run([str(ROOT/'target/release/tend-worker'),'scheduled'],env={**os.environ,'HOME':str(home)},text=True,capture_output=True)
        self.assertEqual(result.returncode,0,result.stderr)
        for text in ['Dry run:','Cleanup preview','npm diagnostic logs:','Sample cache','2 files','3.0 MiB','APT runs only in daily']:
            self.assertIn(text,result.stdout)
        self.assertEqual(before,{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths})
        self.assertFalse((home/'.local/state/tend/scheduled-cleanup.json').exists())
        (stage/'worker-dry-run.txt').write_text(result.stdout)
        print('Retained read-only worker preview:',stage/'worker-dry-run.txt')

    def test_dry_run_install_repeat_and_rollback_preserve_unrelated_settings(self):
        Path('/tmp/opencode').mkdir(exist_ok=True)
        stage=Path(tempfile.mkdtemp(prefix='tend-policy-install-',dir='/tmp/opencode'))
        originals={
            '/usr/local/lib/dev-maintenance/dev-maintenance.py':(ROOT/'tests/fixtures/maintenance-engine.py.txt').read_bytes(),
            '/usr/local/libexec/tend-admin':b'original staged helper\n',
            '/usr/local/libexec/tend-worker':b'original staged worker\n',
            '/etc/tend/settings.json':b'{"enabled":false,"hour":8,"minute":0,"reboot":false}\n',
            '/etc/tend/cleanup-policy.json':b'{"build_days":45,"image_days":90,"keep_gb":12}\n',
            '/etc/systemd/system/dev-maintenance.timer':b'existing timer fixture\n',
            '/etc/systemd/system/dev-maintenance.service.d/reboot.conf':b'existing hook fixture; never executed\n',
        }
        for name,data in originals.items():
            path=stage/name.lstrip('/');path.parent.mkdir(parents=True,exist_ok=True)
            path.write_bytes(data);path.chmod(0o755 if '/libexec/' in name else 0o644)
        for path in stage.rglob('*'):
            if path.is_dir():path.chmod(0o755)
        def hashes():return {str(p.relative_to(stage)):hashlib.sha256(p.read_bytes()).hexdigest() for p in stage.rglob('*') if p.is_file()}
        logs=[]
        def run(*args):
            p=subprocess.run(['python3',str(ROOT/'scripts/install-policy.py'),'--root',str(stage),*args],text=True,capture_output=True)
            self.assertEqual(p.returncode,0,p.stderr);logs.append(p.stdout)
        before=hashes();run('--dry-run');self.assertEqual(before,hashes())
        run('--apply');manifest=next((stage/'etc/tend').glob('policy-install.*.json'))
        engine=stage/'usr/local/lib/dev-maintenance/dev-maintenance.py';patched=engine.read_bytes()
        self.assertIn(b'# Tend cleanup integration v1',patched)
        for name in ('tend-admin','tend-worker'):
            self.assertEqual((stage/'usr/local/libexec'/name).read_bytes(),(ROOT/'target/release'/name).read_bytes())
        for name,data in originals.items():
            if name.startswith('/etc/') or '/systemd/' in name:self.assertEqual((stage/name.lstrip('/')).read_bytes(),data)
        self.assertEqual((stage/'etc/tend/cleanup-policy-ready').read_text(),'1\n')
        run('--apply');self.assertEqual(engine.read_bytes(),patched)
        run('--rollback',str(manifest),'--dry-run');run('--rollback',str(manifest),'--apply')
        for name,data in originals.items():self.assertEqual((stage/name.lstrip('/')).read_bytes(),data)
        self.assertFalse((stage/'etc/tend/cleanup-policy-ready').exists())
        self.assertTrue(list((stage/'etc/tend').glob('cleanup-policy-ready.disabled.*')))
        self.assertTrue(list((stage/'usr/local/lib/dev-maintenance').glob('tend_cleanup_policy.py.disabled.*')))
        unsafe=stage/'usr/local/lib/dev-maintenance'
        unsafe.chmod(0o775)
        rejected=subprocess.run(['python3',str(ROOT/'scripts/install-policy.py'),'--root',str(stage),'--dry-run'],text=True,capture_output=True)
        self.assertNotEqual(rejected.returncode,0);self.assertIn('Unprotected destination',rejected.stderr)
        unsafe.chmod(0o755)
        link=unsafe/'tend_cleanup_policy.py';link.symlink_to(stage/'etc/tend/settings.json')
        rejected=subprocess.run(['python3',str(ROOT/'scripts/install-policy.py'),'--root',str(stage),'--dry-run'],text=True,capture_output=True)
        self.assertNotEqual(rejected.returncode,0);self.assertIn('Unprotected destination',rejected.stderr)
        (stage/'validation.log').write_text('\n'.join(logs))
        evidence=dict(stage=str(stage),dry_run_unchanged=True,install_checksums=True,repeat_install_idempotent=True,settings_timer_hook_preserved=True,rollback_byte_exact=True,no_commands_or_services_started=True,unsafe_paths_rejected=True)
        (stage/'evidence.json').write_text(json.dumps(evidence,indent=2)+'\n')
        print('Retained installer staging evidence:',stage/'evidence.json')

if __name__=='__main__':unittest.main()
