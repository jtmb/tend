"""Safety checks with command execution and cleanup completely mocked."""
import importlib.util
import json
import os
import sys
import types
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock

ROOT = Path(__file__).resolve().parents[1]

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

policy = load('daily_policy', ROOT / 'assets/tend_cleanup_policy.py')
installer = load('policy_installer', ROOT / 'scripts/install-policy.py')

class DailyPolicyTests(unittest.TestCase):
    def setUp(self):
        self.run = Mock(return_value=(0, 'mocked command'))
        self.user = lambda command: ['AS-USER', *command]
        self.apt = Mock(return_value=False)
        self.npm = Mock(return_value=False)

    def execute(self, phase, p=None, state=None, now=10000000):
        return policy.execute(phase, {} if state is None else state,
                              policy.DEFAULTS if p is None else p,
                              self.run, self.user, self.apt, self.npm, now)

    def test_shared_schema_contract(self):
        for case in json.loads((ROOT/'tests/policy-cases.json').read_text()):
            with self.subTest(case=case['name']):
                try:
                    policy.validate(case['policy']);valid=True
                except ValueError:valid=False
                self.assertEqual(valid,case['valid'])

    def test_disabled_categories_never_spawn_commands_or_check_locks(self):
        p={**policy.DEFAULTS, **{v: False for v in policy.PHASES.values()}}
        for phase in policy.PHASES:
            self.assertEqual(self.execute(phase, p), ('success','Skipped by policy'))
        self.run.assert_not_called(); self.apt.assert_not_called(); self.npm.assert_not_called()

    def test_malformed_policy_fails_closed_but_does_not_raise_into_other_phases(self):
        Path('/tmp/opencode').mkdir(exist_ok=True)
        root=Path(tempfile.mkdtemp(prefix='tend-invalid-policy-',dir='/tmp/opencode'))
        path=root/'policy.json';path.write_text('{')
        value=policy.snapshot(path)
        self.assertIsInstance(value,Exception)
        for phase in policy.PHASES:self.assertEqual(self.execute(phase,value)[0],'failed')
        self.run.assert_not_called();self.assertEqual(path.read_text(),'{')
        for raw in ({'version':2},{'apt':1},{'build_days':6},{'npm_log_days':0},{'npm_verify_days':31},{'command':'reboot'}):
            with self.assertRaises(ValueError):policy.validate(raw)

    def test_custom_values_and_one_snapshot_are_passed_to_the_user_worker(self):
        p=policy.validate(dict(build_days=60,image_days=90,keep_gb=9,npm_log_days=45))
        self.assertEqual(self.execute('docker-build',p)[0],'success')
        args=self.run.call_args.args[0]
        self.assertEqual(args[:4],['AS-USER','/usr/local/libexec/tend-worker','daily-phase','docker-build'])
        self.assertEqual(json.loads(args[4]),p)
        self.assertIn('until=1440h --keep-storage 9GB',policy.describe(p))
        self.assertIn('until=2160h',policy.describe(p))

    def test_apt_contention_and_local_offline_cleanup(self):
        self.apt.return_value=True
        self.assertEqual(self.execute('apt-cache')[0],'deferred')
        self.run.assert_not_called()
        self.assertEqual(self.execute('docker-images')[0],'success')
        self.assertEqual(self.run.call_args.args[0][3],'docker-images')

    def test_npm_interval_advances_only_on_success_and_busy_defers(self):
        state={'npm_last_verified':10000000-3*86400}
        self.assertEqual(self.execute('npm-cache',state=state)[0],'success');self.run.assert_not_called()
        p={**policy.DEFAULTS,'npm_verify_days':2}
        self.npm.return_value=True
        self.assertEqual(self.execute('npm-cache',p,state)[0],'deferred');self.run.assert_not_called()
        self.npm.return_value=False;self.run.return_value=(1,'mocked failure')
        old=state.copy();self.assertEqual(self.execute('npm-cache',p,state)[0],'failed');self.assertEqual(old,state)
        self.run.return_value=(0,'mocked success');self.execute('npm-cache',p,state)
        self.assertEqual(state['npm_last_verified'],10000000)

    def test_worker_contention_and_failure_are_not_reported_as_cleanup_success(self):
        for output in ['Another cleanup or schedule change is in progress','fatal Docker error']:
            self.run.return_value=(1,output)
            self.assertIn(self.execute('docker-build')[0],('failed','deferred'))

    def test_integrated_engine_defers_offline_updates_and_preserves_retry_guards(self):
        source=ROOT/'tests/fixtures/maintenance-engine.py.txt'
        module=types.ModuleType('staged_maintenance')
        old=sys.modules.get('tend_cleanup_policy')
        sys.modules['tend_cleanup_policy']=policy
        try:
            exec(compile(installer.patch_engine(source.read_text()),'staged_engine','exec'),module.__dict__)
            module.run_command=self.run;module.as_user=self.user
            module.apt_locks_busy=self.apt;module.npm_busy=self.npm
            module.update_guard=lambda:'Offline (mocked)'
            invalid=ValueError('bad fixture')
            self.assertEqual(module.execute_phase('docker-build',{},10000000,invalid)[0],'failed')
            self.assertEqual(module.execute_phase('updates',{},10000000,invalid)[0],'deferred')
            self.run.assert_not_called()
            entry=dict(window='2026-10-23',success=False,attempts=1,last_attempt=100)
            self.assertFalse(module.phase_due(entry,'2026-10-23',101))
            self.assertTrue(module.phase_due(entry,'2026-10-23',7300))
            self.assertFalse(module.phase_due({**entry,'attempts':3},'2026-10-23',100000))
            self.assertFalse(module.phase_due({**entry,'success':True},'2026-10-23',100000))
            self.assertTrue(module.phase_due(entry,'2026-10-24',100000))
            self.assertEqual(module.boot_grace_remaining(100),1100)
            self.assertEqual(module.boot_grace_remaining(1201),0)
        finally:
            if old is None:sys.modules.pop('tend_cleanup_policy',None)
            else:sys.modules['tend_cleanup_policy']=old

    def test_engine_patch_preserves_schedule_and_non_cleanup_behavior(self):
        # A sanitized supported-engine fixture; compiling executes nothing.
        source=ROOT/'tests/fixtures/maintenance-engine.py.txt'
        old=source.read_text();new=installer.patch_engine(old)
        self.assertEqual(installer.patch_engine(new),new)
        self.assertIn('cleanup_policy = _tend_cleanup.snapshot()',new)
        self.assertIn('phase_status, message = execute_phase(name, state, now, cleanup_policy)',new)
        self.assertEqual(old[old.index('def due_window'):old.index('def phase_due')],new[new.index('def due_window'):new.index('def phase_due')])
        self.assertIn('return _tend_legacy_execute_phase(name, state, now)',new)
        compile(new,'staged-engine','exec')
        with self.assertRaises(ValueError):installer.patch_engine('unsupported engine')

if __name__=='__main__':unittest.main()
