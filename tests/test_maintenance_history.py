"""Durable receipts and bounded history, with fixture-only actions and storage."""
import datetime as dt
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('maintenance_event_tests', ROOT/'assets/tend_maintenance_history.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class MaintenanceHistoryTests(unittest.TestCase):
    def setUp(self):
        Path('/tmp/opencode').mkdir(exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix='tend-maintenance-history-', dir='/tmp/opencode'))
        self.root.chmod(0o700)
        self.now = int(dt.datetime(2026, 10, 26, 12, tzinfo=dt.timezone.utc).timestamp()*1000)
        self.clock = 4000
        self.space = 8_000_000_000

    def store(self):
        return module.PhaseHistory(self.root, owner=os.geteuid(), wall_ms=lambda:self.now,
                                   boot_ms=lambda:self.clock, boot_id=lambda:'fixture-boot',
                                   free=lambda:self.space)

    def test_success_receipt_prevents_replay_after_state_loss(self):
        with self.store() as history:
            ticket, previous = history.begin('docker-build', '2026-10-26', 1, scheduled=self.now-120000)
            self.assertIsNone(previous)
            self.now += 1000; self.clock += 1500; self.space += 2048
            result = history.finish(ticket, 'success', 'Completed fixture')
            self.assertEqual(result['duration_ms'], 1500)
            self.assertEqual(result['free_delta_bytes'], 2048)
            self.assertEqual(result['catch_up_seconds'], 120)
        with self.store() as restored:
            for attempt in (1, 2, 3):
                ticket, previous = restored.begin('docker-build', '2026-10-26', attempt, scheduled=self.now)
                self.assertIsNone(ticket);self.assertEqual(previous[0], 'success')
                self.assertIn('not repeated', previous[1])
            self.assertEqual(restored.db.execute('SELECT count(*) FROM events').fetchone()[0], 2)

    def test_interrupted_or_unwritten_outcome_remains_uncertain(self):
        with self.store() as history:
            ticket, _ = history.begin('apt-cache', '2026-10-26', 1, scheduled=self.now)
            # Simulate the optional finish record failing after the action.
            history.db.execute("CREATE TRIGGER fail_finish BEFORE INSERT ON events WHEN NEW.kind='finished' BEGIN SELECT RAISE(ABORT,'fixture audit failure'); END")
            with self.assertRaises(sqlite3.DatabaseError):
                history.finish(ticket, 'success', 'The action already completed')
            self.assertEqual(history.db.execute('SELECT status FROM receipts').fetchone()[0], 'running')
        with self.store() as reopened:
            ticket, previous = reopened.begin('apt-cache', '2026-10-26', 2, scheduled=self.now)
            self.assertIsNone(ticket);self.assertIn('uncertain', previous[1])
            self.assertEqual(reopened.db.execute('SELECT count(*) FROM events').fetchone()[0], 1)

    def test_deferred_and_failed_phases_retry_but_completed_attempts_are_not_replayed(self):
        with self.store() as history:
            for attempt, status in ((1, 'deferred'), (2, 'failed'), (3, 'success')):
                ticket, previous = history.begin('updates', '2026-10-26', attempt, scheduled=self.now)
                self.assertIsNone(previous)
                history.finish(ticket, status, f'Fixture {status}')
                duplicate, cached = history.begin('updates', '2026-10-26', attempt, scheduled=self.now)
                self.assertIsNone(duplicate);self.assertEqual(cached[0], status)
            self.assertEqual(history.db.execute('SELECT count(*) FROM events').fetchone()[0], 6)
            with self.assertRaises(ValueError):history.begin('updates', '2026-10-26', 4)

    def test_wall_clock_rollback_preserves_monotonic_duration_and_blocks_older_dates(self):
        with self.store() as history:
            ticket, _ = history.begin('health', '2026-10-26', 1, scheduled=self.now)
            self.now -= 3_600_000;self.clock += 500
            result = history.finish(ticket, 'success', 'Clock changed')
            self.assertEqual(result['duration_ms'], 500);self.assertTrue(result['clock_changed'])
            ticket, previous = history.begin('health', '2026-10-25', 1, scheduled=self.now)
            self.assertIsNone(ticket);self.assertIn('earlier maintenance date', previous[1])

    def test_retention_output_bounds_and_quota_are_enforced(self):
        with self.store() as history:
            original = dt.date(2026, 9, 1)
            for i in range(40):
                day = original + dt.timedelta(days=i)
                self.now = int(dt.datetime.combine(day, dt.time(12), dt.timezone.utc).timestamp()*1000)
                for phase in module.PHASES:
                    for attempt in (1,2,3):
                        ticket, _ = history.begin(phase, day.isoformat(), attempt, scheduled=self.now)
                        history.finish(ticket, 'success' if attempt==3 else 'failed', 'é'*4000)
            self.assertEqual(history.db.execute('SELECT count(*) FROM events').fetchone()[0], module.MAX_EVENTS)
            self.assertGreaterEqual(history.db.execute('SELECT min(time_ms) FROM events').fetchone()[0], self.now-30*86400*1000)
            row = json.loads(history.db.execute("SELECT payload FROM events WHERE kind='finished' LIMIT 1").fetchone()[0])
            self.assertLessEqual(len(row['message'].encode()), module.MESSAGE_BYTES+20)
            self.assertIn('[truncated]', row['message'])
            self.assertNotIn('_boot_ms', row)
            page_size = history.db.execute('PRAGMA page_size').fetchone()[0]
            pages = history.db.execute('PRAGMA max_page_count').fetchone()[0]
            self.assertLessEqual(page_size*pages, module.MAX_BYTES)
            self.assertLessEqual(history.path.stat().st_size, module.MAX_BYTES)

    def test_corrupt_symlink_and_unsafe_files_are_preserved(self):
        path = self.root/module.FILENAME
        path.write_bytes(b'corrupt fixture');path.chmod(0o644)
        with self.assertRaises(sqlite3.DatabaseError):self.store()
        self.assertEqual(path.read_bytes(), b'corrupt fixture')
        retained = self.root/'retained-corrupt';path.rename(retained);path.symlink_to(retained)
        with self.assertRaises(OSError):self.store()
        path.rename(self.root/'retained-link');path.write_text('');path.chmod(0o666)
        with self.assertRaises(ValueError):self.store()

    def test_unrecognized_database_is_not_migrated_or_overwritten(self):
        path=self.root/module.FILENAME
        with sqlite3.connect(path) as db:
            db.executescript('CREATE TABLE unrelated(value); PRAGMA user_version=1;')
        path.chmod(0o644);before=path.read_bytes()
        with self.assertRaisesRegex(ValueError,'Unrecognized'):self.store()
        self.assertEqual(path.read_bytes(),before)

    def test_phase_wrapper_does_not_repeat_action_when_history_fails_after_success(self):
        action = Mock(return_value=('success', 'Fixture action completed'))
        real_type = module.PhaseHistory
        def store_factory(_path):
            value = real_type(self.root, owner=os.geteuid(), wall_ms=lambda:self.now,
                              boot_ms=lambda:self.clock, boot_id=lambda:'fixture-boot', free=lambda:self.space)
            value.db.execute("CREATE TRIGGER IF NOT EXISTS fail_finish BEFORE INSERT ON events WHEN NEW.kind='finished' BEGIN SELECT RAISE(ABORT,'fixture audit failure'); END")
            return value
        with patch.object(module, 'PhaseHistory', side_effect=store_factory), patch.object(module, 'scheduled_ms', return_value=self.now):
            result = module.run_phase(self.root, 'docker-images', '2026-10-26', 1, action)
            self.assertEqual(result[0], 'success');self.assertIn('history unavailable', result[1])
            second = module.run_phase(self.root, 'docker-images', '2026-10-26', 2, action)
            self.assertEqual(second[0], 'deferred');self.assertIn('uncertain', second[1])
        action.assert_called_once()

    def test_rollback_requires_known_legacy_state_before_removing_receipts(self):
        with self.store() as history:
            ticket,_=history.begin('apt-cache','2026-10-26',1,scheduled=self.now)
            with self.assertRaisesRegex(ValueError,'uncertain'):
                module.check_rollback(self.root,owner=os.geteuid())
            history.finish(ticket,'success','Fixture action completed')
            with self.assertRaisesRegex(ValueError,'Reconcile'):
                module.check_rollback(self.root,owner=os.geteuid())
            state=self.root/'state.json'
            state.write_text(json.dumps(dict(phases={'apt-cache':dict(window='2026-10-26',success=True)})))
            state.chmod(0o600)
            module.check_rollback(self.root,owner=os.geteuid())


if __name__ == '__main__':unittest.main()
