"""Worker smoke/regression checks using retained homes and fake npm executables.

No real npm cache, Docker, package, cron, or service command is executed.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
BINARY=ROOT/'target/debug/tend-worker'
POLICY=dict(version=1,apt=True,docker_build=True,docker_images=True,npm_logs=True,
            npm_verify=True,build_days=14,image_days=14,keep_gb=5,
            npm_log_days=30,npm_verify_days=7)


@unittest.skipUnless(BINARY.is_file(),'Build debug tend-worker first')
class WorkerContracts(unittest.TestCase):
    def setUp(self):
        Path('/tmp/opencode').mkdir(exist_ok=True)
        self.root=Path(tempfile.mkdtemp(prefix='tend-worker-qa-',dir='/tmp/opencode'))
        self.home=self.root/'home';self.home.mkdir()
        self.bin=self.root/'bin';self.bin.mkdir()
        self.env={k:v for k,v in os.environ.items() if not k.startswith('TEND_')}
        self.env.update(HOME=str(self.home),PATH=str(self.bin)+':/usr/bin:/bin',
                        XDG_CONFIG_HOME=str(self.home/'.config'),
                        XDG_DATA_HOME=str(self.home/'.local/share'),
                        XDG_CACHE_HOME=str(self.home/'.cache'),
                        DBUS_SESSION_BUS_ADDRESS='unix:path='+str(self.root/'no-bus'))

    def script(self,path,body):
        path.parent.mkdir(parents=True,exist_ok=True)
        path.write_text(body);path.chmod(0o755)

    def run_worker(self,*args,data='',env=None):
        result=subprocess.run([str(BINARY),*args],input=data,text=True,capture_output=True,
                              env=self.env if env is None else env,timeout=30)
        with (self.root/'results.jsonl').open('a') as f:
            f.write(json.dumps(dict(args=args,code=result.returncode,stdout=result.stdout,stderr=result.stderr))+'\n')
        return result

    def tearDown(self):
        print('Retained worker contract fixture:',self.root)

    def test_daily_npm_uses_path_without_opencode_installation(self):
        self.script(self.bin/'npm','#!/bin/sh\nprintf "fixture npm: %s %s\\n" "$1" "$2"\n')
        result=self.run_worker('daily-phase','npm-cache',json.dumps(POLICY))
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn('fixture npm: cache verify',result.stdout)
        reports=list((self.home/'.local/state/tend/reports').glob('*.json'))
        self.assertEqual(len(reports),1)
        self.assertEqual(json.loads(reports[0].read_text())['kind'],'npm-verify')

    def test_opencode_npm_has_its_sibling_node_on_path(self):
        npm=self.home/'.opencode/bin/npm'
        self.script(npm,'#!/usr/bin/env node\n// Not executed by real Node\n')
        self.script(npm.parent/'node','#!/bin/sh\nprintf "fixture sibling node\\n"\n')
        result=self.run_worker('daily-phase','npm-cache',json.dumps(POLICY))
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn('fixture sibling node',result.stdout)

    def test_npm_failure_and_disabled_phase_are_not_success(self):
        self.script(self.bin/'npm','#!/bin/sh\nprintf "fixture failure\\n" >&2\nexit 7\n')
        result=self.run_worker('daily-phase','npm-cache',json.dumps(POLICY))
        self.assertNotEqual(result.returncode,0)
        self.assertIn('fixture failure',result.stderr)
        self.assertNotEqual(self.run_worker('daily-phase','npm-cache',json.dumps({**POLICY,'npm_verify':False})).returncode,0)
        self.assertNotEqual(self.run_worker('daily-phase','unknown',json.dumps(POLICY)).returncode,0)

    def test_unknown_missing_and_invalid_input_are_rejected(self):
        for args,data in [(('unknown',),''),(('cleanup',),'not-json'),
                          (('daily-phase',),''),
                          (('daily-phase','npm-cache','{"version":2}'),'')]:
            with self.subTest(args=args):
                self.assertNotEqual(self.run_worker(*args,data=data).returncode,0)
        self.assertEqual(self.run_worker('scheduled').stdout.strip(),'Cleanup schedule disabled.')
        self.assertFalse((self.home/'.local/state/tend/scheduled-cleanup.json').exists())


if __name__=='__main__':unittest.main()
