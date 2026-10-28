import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('tend_install_checks',Path(__file__).resolve().parents[1]/'scripts/install_checks.py')
checks=importlib.util.module_from_spec(spec);spec.loader.exec_module(checks)

class IntegrationPreflightTests(unittest.TestCase):
    def capabilities(self,role):
        return dict(protocol=4,role=role,cleanup_exclusions=True,explained_cleanup=True,maintenance_events=True)
    def test_all_roles_and_strict_flags(self):
        for _,role,_ in checks.COMPONENTS:
            good=self.capabilities(role);checks.validate_capabilities(good,role)
            for key,value in [('protocol',True),('protocol',3),('role','other'),('maintenance_events',1),('maintenance_events',False)]:
                with self.subTest(role=role,key=key,value=value):
                    with self.assertRaises(ValueError):checks.validate_capabilities(good|{key:value},role)
            if role!='maintenance-history':
                for key in ('cleanup_exclusions','explained_cleanup'):
                    bad=good.copy();bad.pop(key)
                    with self.assertRaises(ValueError):checks.validate_capabilities(bad,role)
    def test_trust_rejects_symlinks_owners_and_writable_ancestors(self):
        import stat
        from types import SimpleNamespace as Info
        good_file=Info(st_mode=stat.S_IFREG|0o755,st_uid=0,st_gid=0)
        good_directory=Info(st_mode=stat.S_IFDIR|0o755,st_uid=0,st_gid=0)
        for bad in [Info(st_mode=stat.S_IFLNK|0o777,st_uid=0,st_gid=0),Info(st_mode=stat.S_IFREG|0o775,st_uid=0,st_gid=0),Info(st_mode=stat.S_IFREG|0o755,st_uid=1000,st_gid=0)]:
            with patch.object(Path,'lstat',return_value=bad):
                with self.assertRaises(ValueError):checks.trusted('/trusted/helper')
        for bad in [Info(st_mode=stat.S_IFDIR|0o777,st_uid=0,st_gid=0),Info(st_mode=stat.S_IFDIR|0o775,st_uid=0,st_gid=1000),Info(st_mode=stat.S_IFLNK|0o755,st_uid=0,st_gid=0)]:
            with patch.object(Path,'lstat',side_effect=[good_file,bad]):
                with self.assertRaises(ValueError):checks.trusted('/trusted/helper')
        with patch.object(Path,'lstat',side_effect=[good_file,good_directory,good_directory]):checks.trusted('/trusted/helper')

    def test_complete_preflight_and_bounded_readonly_commands(self):
        import json
        calls=[]
        def run(command,**kwargs):
            calls.append(command);self.assertEqual(kwargs['timeout'],3);self.assertEqual(kwargs['stdin'],subprocess.DEVNULL)
            self.assertTrue(kwargs['capture_output']);self.assertFalse(kwargs['check'])
            role=checks.COMPONENTS[len(calls)-1][1]
            return subprocess.CompletedProcess(command,0,json.dumps(self.capabilities(role)).encode(),b'')
        with patch.object(checks,'trusted') as trust,patch.object(checks.subprocess,'run',side_effect=run),patch.object(Path,'stat') as stat,patch.object(Path,'read_bytes',return_value=b'1\n'):
            stat.return_value.st_size=2;checks.check_integration();self.assertEqual(trust.call_count,7)
        self.assertEqual(len(calls),4)
        self.assertTrue(all(command[-1] in ('capabilities','--capabilities') for command in calls))
    def test_timeout_failure_oversize_and_incomplete_marker(self):
        for result in [subprocess.CompletedProcess([],1,b'',b''),subprocess.CompletedProcess([],0,b' '*8193,b'')]:
            with patch.object(checks,'trusted'),patch.object(checks.subprocess,'run',return_value=result):
                with self.assertRaises(ValueError):checks.check_integration()
        with patch.object(checks,'trusted'),patch.object(checks.subprocess,'run',side_effect=subprocess.TimeoutExpired([],3)):
            with self.assertRaises(subprocess.TimeoutExpired):checks.check_integration()
        with patch.object(checks,'COMPONENTS',()),patch.object(checks,'trusted'),patch.object(Path,'stat') as stat,patch.object(Path,'read_bytes',return_value=b'0'):
            stat.return_value.st_size=1
            with self.assertRaises(ValueError):checks.check_integration()

if __name__=='__main__':unittest.main()
