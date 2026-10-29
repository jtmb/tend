"""No compiler, memory stress or desktop process is started by these checks."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('limited_build', Path(__file__).resolve().parents[1] / 'scripts/limited-build.py')
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class BuildGuardTests(unittest.TestCase):
    def test_start_reserves_ram_and_swap_and_defers_large_node_work(self):
        healthy = dict(MemAvailable=3 * guard.GIB, SwapTotal=4 * guard.GIB, SwapFree=2 * guard.GIB)
        self.assertIsNone(guard.reason(healthy, [], True))
        self.assertIn('reserve', guard.reason({**healthy, 'MemAvailable': 1600 * guard.MIB}, [], True))
        self.assertIn('Swap', guard.reason({**healthy, 'MemAvailable': 2300 * guard.MIB, 'SwapFree': 24 * 1024}, [], True))
        self.assertIsNone(guard.reason({**healthy, 'SwapFree': 24 * 1024}, [], True))
        self.assertIn('46694', guard.reason(healthy, [(46694, 4 * guard.GIB)], True))
        self.assertIsNone(guard.reason({**healthy, 'SwapTotal': 0, 'SwapFree': 0}, [], True))

    def test_runtime_interrupt_targets_only_the_owned_transient_scope(self):
        calls = []
        guard.stop_owned_scope('tend-build-fixture.scope', runner=lambda *a, **k: calls.append((a, k)))
        self.assertEqual(calls[0][0][0][-1], 'tend-build-fixture.scope')
        self.assertIn('--signal=SIGINT', calls[0][0][0])
        self.assertIn('--no-ask-password', calls[0][0][0])
        with self.assertRaises(ValueError):
            guard.stop_owned_scope('app-opencode.scope', runner=lambda *a, **k: self.fail('Unrelated process was targeted'))

    def test_launch_race_or_manager_failure_still_interrupts_only_owned_group(self):
        class Child:
            pid = 4242
            def poll(self): return None
            def wait(self, timeout): return 0
        with patch.object(guard.os, 'killpg') as kill, patch.object(guard, 'stop_owned_scope', side_effect=OSError('private bus unavailable')):
            guard.interrupt(Child(), 'tend-build-fixture.scope')
        kill.assert_called_once_with(4242, guard.signal.SIGINT)


if __name__ == '__main__':
    unittest.main()
