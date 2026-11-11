"""Regression coverage for process-owned quotas. No Hermes/provider import is required."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("quota", Path(__file__).with_name("delegation_quota.py"))
quota = importlib.util.module_from_spec(spec)
spec.loader.exec_module(quota)


class QuotaTests(unittest.TestCase):
    def test_last_running_child_keeps_its_slot_until_it_releases_it(self):
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            first = quota.acquire(directory, 1)
            self.assertIsNotNone(first)
            self.assertIsNone(quota.acquire(directory, 1))
            first.close()
            second = quota.acquire(directory, 1)
            self.assertIsNotNone(second)
            second.close()

    def test_owner_process_exit_releases_capacity_without_stale_pid_cleanup(self):
        with tempfile.TemporaryDirectory() as raw:
            script = """
import importlib.util, pathlib, sys
spec=importlib.util.spec_from_file_location('quota',sys.argv[1])
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
lease=module.acquire(pathlib.Path(sys.argv[2]),1)
assert lease is not None
print('locked',flush=True)
sys.stdin.read()
"""
            process = subprocess.Popen([sys.executable, "-B", "-c", script, str(Path(quota.__file__)), raw],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
            try:
                self.assertEqual(process.stdout.readline().strip(), "locked")
                self.assertIsNone(quota.acquire(Path(raw), 1))
            finally:
                process.terminate()
                process.wait(timeout=5)
                process.stdin.close()
                process.stdout.close()
            lease = quota.acquire(Path(raw), 1)
            self.assertIsNotNone(lease)
            lease.close()


if __name__ == "__main__":
    unittest.main()
