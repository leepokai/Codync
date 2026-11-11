"""Native-config seam regressions, using isolated config/provider-free test doubles."""
import copy
import importlib.util
from pathlib import Path
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("owner", Path(__file__).with_name("acp_owner.py"))
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)


def deep_merge(base, override):
    result = copy.deepcopy(base)
    for key, value in override.items():
        result[key] = deep_merge(result[key], value) if isinstance(result.get(key), dict) and isinstance(value, dict) else copy.deepcopy(value)
    return result


class OverlayTests(unittest.TestCase):
    def test_new_provider_drops_old_request_body_without_rewriting_profile(self):
        home = Path(".").resolve()
        native = {"delegation": {"provider": "old", "request_overrides": {"old-route-only": True}}}
        config = types.SimpleNamespace(load_config=lambda: copy.deepcopy(native), load_config_readonly=lambda: copy.deepcopy(native), _deep_merge=deep_merge)
        package = types.ModuleType("hermes_cli")
        package.config = config
        package.config_backend = types.SimpleNamespace(get_config_backend=lambda: types.SimpleNamespace(locked=lambda *_: False, honors_managed_config=lambda: True))
        package.managed_scope = types.SimpleNamespace(apply_managed_overlay=lambda value: value)
        constants = types.ModuleType("hermes_constants")
        constants.get_hermes_home = lambda: home
        with patch.dict("sys.modules", {"hermes_cli": package, "hermes_constants": constants}):
            owner.install_overlay(home, {"delegation": {"provider": "new", "request_overrides": {}}})
            self.assertEqual(config.load_config()["delegation"], {"provider": "new", "request_overrides": {}})
            self.assertEqual(config.load_config_readonly()["delegation"]["request_overrides"], {})
        self.assertEqual(native["delegation"]["provider"], "old")
        self.assertEqual(native["delegation"]["request_overrides"], {"old-route-only": True})


if __name__ == "__main__":
    unittest.main()
