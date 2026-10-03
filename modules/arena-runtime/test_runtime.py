import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import runtime


class FakeProcess:
    pid = 4321
    stdout = None

    def __init__(self):
        self.returncode = None

    def poll(self):
        return self.returncode

    def send_signal(self, _signal):
        self.returncode = 0

    def wait(self, timeout=None):
        self.returncode = 0
        return self.returncode


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.runtime = runtime.ArenaRuntime()
        self.data_dir = Path(self.temp.name) / "data"
        self.data_dir.mkdir()
        self.arena_bin = Path(self.temp.name) / "server"
        self.arena_bin.touch()
        self.worlds_path = Path(__file__).resolve().parents[2] / "assets" / "worlds.json"

    def tearDown(self):
        self.temp.cleanup()

    def request(self, **overrides):
        return {
            "run_id": "a" * 32,
            "world_id": "quiet-harbor-01",
            "arena_name": "world_quiet-harbor-01_1_1",
            "observer_token": "private-observer-token",
            "duration_sec": 1200,
            **overrides,
        }

    def test_start_builds_runtime_contract_from_world_catalog_and_stops_child(self):
        process = FakeProcess()
        with patch.multiple(runtime, DATA_DIR=self.data_dir, WORLDS_PATH=self.worlds_path,
                            ARENA_BIN=self.arena_bin), \
             patch.object(runtime.subprocess, "Popen", return_value=process) as popen:
            result = self.runtime.start(self.request())
            self.assertEqual(result["status"], "running")
            self.assertEqual(result["pid"], process.pid)
            env = popen.call_args.kwargs["env"]
            self.assertEqual(env["DATS_WORLD_ID"], "quiet-harbor-01")
            self.assertEqual(env["DATS_WORLD_RUN_NAME"], "world_quiet-harbor-01_1_1")
            self.assertEqual(env["DATS_TOKEN_REGISTRY_PATH"], str(self.data_dir / "registry.json"))
            self.assertEqual(env["DATS_LEADERBOARD_PATH"], str(self.data_dir / "arena" / f"run_{'a' * 32}" / "metrics.json"))
            self.assertTrue(self.runtime.status("a" * 32)["running"])
            stopped = self.runtime.stop("a" * 32)
            self.assertFalse(stopped["running"])
            self.assertEqual(stopped["exit_code"], 0)

    def test_unknown_world_is_rejected_before_starting_rust_process(self):
        with patch.multiple(runtime, DATA_DIR=self.data_dir, WORLDS_PATH=self.worlds_path,
                            ARENA_BIN=self.arena_bin), \
             patch.object(runtime.subprocess, "Popen") as popen:
            with self.assertRaisesRegex(ValueError, "world_id"):
                self.runtime.start(self.request(world_id="missing-world"))
            popen.assert_not_called()


if __name__ == "__main__":
    unittest.main()
