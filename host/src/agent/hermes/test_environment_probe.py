"""Health-probe regressions with fake backends; no SDK, SSH or inference required."""
from __future__ import annotations

import importlib.util
import logging
from pathlib import Path
import sys
import unittest
from unittest import mock
from types import SimpleNamespace

_directory = Path(__file__).parent
_owner_spec = importlib.util.spec_from_file_location("acp_owner", _directory / "acp_owner.py")
_owner = importlib.util.module_from_spec(_owner_spec)
_owner_spec.loader.exec_module(_owner)
with mock.patch.dict(sys.modules, {"acp_owner": _owner}):
    _spec = importlib.util.spec_from_file_location("codync_environment_probe", _directory / "environment_probe.py")
    probe = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(probe)


class EnvironmentProbeTests(unittest.TestCase):
    def modules(self, create, cleanup=None):
        backend = SimpleNamespace(
            _create_environment=create,
            _container_config_from_config=lambda config: dict(config),
            _local_config_from_config=lambda config: {"persistent": config.get("local_persistent", True)},
            _ssh_config_from_config=lambda config: {"host": config.get("ssh_host"), "key": config.get("ssh_key")},
        )
        return {
            "tools": SimpleNamespace(terminal_tool_backends=backend),
            "tools.terminal_tool_backends": backend,
            "tools.terminal_tool_config": SimpleNamespace(_is_container_backend=lambda name: name in {"docker", "modal", "daytona", "vercel_sandbox"}),
            "tools.terminal_tool_lifecycle": SimpleNamespace(_cleanup_env=cleanup),
        }

    def test_all_backends_use_native_factory_with_unique_probe_task_and_existing_policy(self):
        for backend in sorted(probe.BACKENDS):
            with self.subTest(backend=backend):
                create = mock.Mock(return_value=object())
                config = {"cwd": "/project", "host_cwd": "/host-project", "ssh_host": "saved-alias", "ssh_key": "saved-key",
                          "docker_network": False, "docker_env": {"POLICY": "preserved"}, "docker_volumes": ["/allowed:/allowed:ro"],
                          "docker_shared_container_key": "live-bot", "docker_orphan_reaper": True, "container_persistent": True,
                          "modal_mode": "managed", "vercel_runtime": "python3.13", "docker_image": "saved-image"}
                with mock.patch.dict(sys.modules, self.modules(create)):
                    probe.build_probe(backend, config, "codync-health-unique")
                call = create.call_args.kwargs
                self.assertEqual(call["env_type"], backend)
                self.assertEqual(call["task_id"], "codync-health-unique")
                self.assertTrue(call["probe_only"])
                if call["container_config"] is not None:
                    actual = call["container_config"]
                    for key in ["docker_network", "docker_env", "docker_volumes", "modal_mode", "vercel_runtime"]:
                        self.assertEqual(actual[key], config[key])
                    self.assertFalse(actual["container_persistent"])
                    self.assertFalse(actual["docker_orphan_reaper"])
                    self.assertFalse(actual["docker_persist_across_processes"])
                    self.assertEqual(actual["docker_shared_container_key"], "")
                if backend == "ssh":
                    self.assertEqual(call["ssh_config"], {"host": "saved-alias", "key": "saved-key"})
                self.assertTrue(config["docker_orphan_reaper"], "the saved policy object must not mutate")

    def test_cleanup_removes_only_environments_constructed_by_this_probe(self):
        cleanup = mock.Mock()
        first, second = SimpleNamespace(), SimpleNamespace()
        with mock.patch.dict(sys.modules, self.modules(mock.Mock(), cleanup)):
            self.assertTrue(probe.cleanup_probe([first, second]))
        self.assertEqual(cleanup.call_args_list, [mock.call(second, force_remove=True), mock.call(first, force_remove=True)])

    def test_native_background_cleanup_timeout_cannot_report_healthy(self):
        cleanup = mock.Mock()
        environment = SimpleNamespace(wait_for_cleanup=mock.Mock(return_value=False))
        with mock.patch.dict(sys.modules, self.modules(mock.Mock(), cleanup)):
            self.assertFalse(probe.cleanup_probe([environment]))

    def test_native_swallowed_cleanup_warning_cannot_report_healthy(self):
        def cleanup(*_args, **_kwargs):
            logging.getLogger().warning("native cleanup failed")
        with mock.patch.dict(sys.modules, self.modules(mock.Mock(), cleanup)):
            self.assertFalse(probe.cleanup_probe([object()]))

    def test_partial_native_construction_is_still_owned_for_cleanup(self):
        class Native:
            def __init__(self):
                raise RuntimeError("backend constructor failed after allocation")
        module = SimpleNamespace(Environment=Native)
        owned = []
        probe.track_constructor(module, "Environment", owned)
        with self.assertRaises(RuntimeError):
            module.Environment()
        self.assertEqual(len(owned), 1)
        self.assertIsInstance(owned[0], Native)

    def test_execution_and_health_share_the_same_terminal_policy_builder(self):
        self.assertIs(probe.terminal_policy, _owner.terminal_policy)


if __name__ == "__main__":
    unittest.main()
