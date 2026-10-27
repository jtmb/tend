"""Retained fixture policies; every maintenance command is mocked."""
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('tend_exclusion_test_policy',ROOT/'assets/tend_cleanup_policy.py')
policy=importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)


class ExclusionTests(unittest.TestCase):
    def setUp(self):
        Path('/tmp/opencode').mkdir(exist_ok=True)
        self.root=Path(tempfile.mkdtemp(prefix='tend-exclusion-contract-',dir='/tmp/opencode'))
        self.root.chmod(0o700)
        self.config=self.root/'exclusions.json'
        self.lock=self.root/'exclusions.lock'
        self.ready=self.root/'exclusions-ready'
        overrides=patch.multiple(policy,EXCLUSIONS=self.config,EXCLUSION_LOCK=self.lock,
                                  EXCLUSION_READY=self.ready,EXCLUSION_OWNER=os.geteuid())
        overrides.start();self.addCleanup(overrides.stop)
        self.commands=Mock(return_value=(0,'mocked operation'))
        self.apt=Mock(return_value=False);self.npm=Mock(return_value=False)

    def save(self,paths):
        self.config.write_text(json.dumps(dict(version=1,revision=3,paths=[dict(path=str(p),enabled=True,note='') for p in paths])))
        self.config.chmod(0o644)
        self.lock.touch();self.lock.chmod(0o644)

    def execute(self,name,state=None,settings=None):
        return policy.execute(name,{} if state is None else state,settings or policy.DEFAULTS,
                              self.commands,lambda args:['AS-USER',*args],self.apt,self.npm,10_000_000)

    def test_shared_rust_python_schema_contract(self):
        for case in json.loads((ROOT/'tests/exclusion-cases.json').read_text()):
            with self.subTest(case=case['name']):
                try:policy.validate_exclusions(case['policy']);valid=True
                except (ValueError,TypeError):valid=False
                self.assertEqual(valid,case['valid'])
        for value in [dict(paths=[dict(path='/a',enabled=True,note='x'*121)]),
                      dict(paths=[dict(path=f'/a/{i}',enabled=True,note='') for i in range(65)])]:
            with self.assertRaises(ValueError):policy.validate_exclusions(value)

    def test_legacy_absence_is_empty_but_partial_installations_fail_closed(self):
        with policy.exclusion_guard() as saved:self.assertEqual(saved['paths'],[])
        self.save([self.root/'keep'])
        original=self.config.read_bytes()
        self.lock.rename(self.root/'retained-lock')
        self.assertEqual(self.execute('docker-images')[0],'failed')
        self.assertEqual(self.config.read_bytes(),original);self.commands.assert_not_called()
        self.config.rename(self.root/'retained-policy')
        self.ready.write_text('1\n')
        self.assertEqual(self.execute('apt-cache')[0],'failed')
        self.lock.touch();self.lock.chmod(0o644)
        self.assertEqual(self.execute('npm-logs')[0],'failed')

    def test_invalid_policy_and_permissions_are_preserved(self):
        self.save([]);self.config.write_text('{broken')
        self.assertEqual(self.execute('docker-build')[0],'failed')
        self.assertEqual(self.config.read_text(),'{broken')
        self.save([]);self.config.chmod(0o666)
        self.assertEqual(self.execute('npm-logs')[0],'failed')
        self.config.chmod(0o644);self.root.chmod(0o775)
        self.assertEqual(self.execute('npm-logs')[0],'failed')
        self.root.chmod(0o700)
        self.config.rename(self.root/'original.json');self.config.symlink_to(self.root/'original.json')
        self.assertEqual(self.execute('npm-logs')[0],'failed');self.commands.assert_not_called()

    def test_readers_share_lock_and_exclusive_writer_defers_execution(self):
        self.save([])
        with policy.exclusion_guard(),policy.exclusion_guard(),self.lock.open() as lock:
            with self.assertRaises(BlockingIOError):fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        with self.lock.open() as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            self.assertEqual(self.execute('docker-build')[0],'failed')
        self.commands.assert_not_called()

    def test_bulk_operations_skip_without_advancing_npm_verification(self):
        self.save([self.root/'protected'])
        state={'npm_last_verified':123}
        for phase in ('docker-build','docker-images','npm-cache'):
            status,message=self.execute(phase,state)
            self.assertEqual(status,'success');self.assertIn('Skipped by exclusions',message)
        self.assertEqual(state,{'npm_last_verified':123})
        self.commands.assert_not_called();self.npm.assert_not_called();self.apt.assert_not_called()

    def test_apt_uses_actual_cache_and_canonical_aliases(self):
        cache=self.root/'actual-apt';cache.mkdir()
        alias=self.root/'alias';alias.symlink_to(cache,target_is_directory=True)
        self.save([alias/'missing'/'package.deb'])
        value=f"TEND_ARCHIVES='{cache}'"
        self.commands.return_value=(0,value)
        self.assertIn('Skipped by exclusions',self.execute('apt-cache')[1])
        self.assertEqual(self.commands.call_count,1)
        self.assertEqual(self.commands.call_args.args[0],['/usr/bin/apt-config','shell','TEND_ARCHIVES','Dir::Cache::archives/d'])
        self.commands.reset_mock();self.save([self.root/'unrelated'])
        self.commands.side_effect=[(0,value),(0,'mocked autoclean')]
        self.assertEqual(self.execute('apt-cache'),('success','mocked autoclean'))
        self.assertEqual(self.commands.call_args.args[0],['/usr/bin/apt-get','-o','DPkg::Lock::Timeout=0','autoclean'])

    def test_unknown_apt_location_never_runs_cleanup(self):
        self.save([self.root/'protected'])
        for value in ['','TEND_ARCHIVES=relative','TEND_ARCHIVES=/tmp other=1',"TEND_ARCHIVES='"]:
            self.commands.reset_mock();self.commands.return_value=(0,value)
            status,message=self.execute('apt-cache')
            self.assertIn(status,('success','failed'))
            self.assertTrue('Skipped' in message or 'unavailable' in message)
            self.assertEqual(self.commands.call_count,1)

    def test_disabled_phase_does_not_require_a_usable_exclusion_policy(self):
        self.save([]);self.config.write_text('{broken')
        settings={**policy.DEFAULTS,'docker_build':False}
        self.assertEqual(self.execute('docker-build',settings=settings),('success','Skipped by policy'))
        self.commands.assert_not_called()

    def test_preview_and_capability_queries_do_not_mutate_policy(self):
        self.save([self.root/'protected'])
        before=self.config.read_bytes()
        self.commands.return_value=(0,f"TEND_ARCHIVES='{self.root}/apt'")
        report=policy.describe(policy.DEFAULTS,self.commands)
        self.assertIn('docker-build: Skipped by exclusions',report)
        self.assertIn('npm-cache: Skipped by exclusions',report)
        self.assertIn('protected paths omitted',report)
        self.assertEqual(self.config.read_bytes(),before)
        result=subprocess.run(['/usr/bin/python3',str(ROOT/'assets/tend_cleanup_policy.py'),'--capabilities'],capture_output=True,text=True,timeout=3,check=True)
        capabilities=json.loads(result.stdout)
        self.assertEqual(capabilities['role'],'maintenance-adapter')
        self.assertTrue(capabilities['cleanup_exclusions'])


if __name__=='__main__':unittest.main()
