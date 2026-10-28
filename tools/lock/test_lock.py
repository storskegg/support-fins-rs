#!/usr/bin/env python3
"""Tests for lock.py: python3 tools/lock/test_lock.py (a few seconds; uses a private lock dir)."""
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest

LOCK = os.path.join(os.path.dirname(os.path.abspath(__file__)), "lock.py")


class LockTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.env = dict(os.environ, SFRS_LOCK_DIR=os.path.join(self.tmp.name, "locks"),
                        SFRS_LOCK_POLL="0.1", SFRS_LOCK_WARN="1", SFRS_BASELINE_SECS="1", SFRS_BASELINE_SAMPLES="3")
        self.env.pop("SFRS_LOCK_HELD", None)

    def tearDown(self):
        self.tmp.cleanup()

    def run_lock(self, *args, **kw):
        return subprocess.Popen([sys.executable, LOCK, *args], env=self.env, cwd=self.tmp.name,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, **kw)

    def test_exit_code_and_release(self):
        p = self.run_lock("t", "--", "sh", "-c", "exit 7")
        p.communicate(timeout=20)
        self.assertEqual(p.returncode, 7)
        self.assertFalse(os.path.exists(os.path.join(self.env["SFRS_LOCK_DIR"], "machine.d")))

    def test_one_at_a_time_and_waiting_message(self):
        a = self.run_lock("first", "--", "sleep", "3")
        time.sleep(0.8)
        b = self.run_lock("second", "--", "echo", "ran")
        out, err = b.communicate(timeout=30)
        a.communicate(timeout=30)
        self.assertIn("ran", out)
        self.assertIn("waiting for `first`", err)

    def test_stale_lock_is_reclaimed(self):
        d = os.path.join(self.env["SFRS_LOCK_DIR"], "machine.d")
        os.makedirs(d)
        dead = subprocess.Popen(["true"])
        dead.wait()
        with open(os.path.join(d, "holder.json"), "w") as f:
            json.dump({"pid": dead.pid, "start": "Mon Jan  1 00:00:00 2001", "name": "ghost", "cwd": "/x", "since": time.time()}, f)
        p = self.run_lock("t", "--", "echo", "ok")
        out, err = p.communicate(timeout=20)
        self.assertEqual(out.strip(), "ok")
        self.assertIn("reclaiming the stale lock", err)

    def test_nested_passes_through(self):
        p = self.run_lock("outer", "--", sys.executable, LOCK, "inner", "--", "echo", "nested")
        out, _ = p.communicate(timeout=20)
        self.assertEqual(p.returncode, 0)
        self.assertIn("nested", out)

    def test_bench_records_load(self):
        p = self.run_lock("--bench", "b", "--", "sh", "-c", 'cat "$SFRS_BENCH_LOAD_FILE"')
        out, err = p.communicate(timeout=30)
        self.assertIn("baseline", out)
        self.assertIn("baseline load", err)
        self.assertIn("mean during", err)


if __name__ == "__main__":
    unittest.main()
