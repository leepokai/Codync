"""Source: Hermes acp_adapter/{entry,server,session}.py, MIT, 10d25f8a.
Adaptation: one host-owned ACP execution process with read-only per-bot settings.
"""

from __future__ import annotations

import argparse
import asyncio
import copy
import importlib.util
import json
import logging
import os
from pathlib import Path
import sys
import threading


def source_root() -> Path:
    parser = argparse.ArgumentParser()
    parser.add_argument("--hermes-root", default=os.environ.get("CODYNC_HERMES_ROOT"))
    args = parser.parse_args()
    if not args.hermes_root:
        parser.error("--hermes-root is required")
    root = Path(args.hermes_root).expanduser().resolve(strict=True)
    if not (root / "acp_adapter" / "server.py").is_file():
        raise ValueError("The selected Hermes installation has no ACP adapter")
    sys.path.insert(0, str(root))
    return root


def settings() -> dict:
    raw = os.environ.get("CODYNC_HERMES_SETTINGS", "{}")
    if len(raw.encode("utf-8")) > 64 * 1024:
        raise ValueError("Hermes settings exceed 64 KiB")
    value = json.loads(raw)
    if not isinstance(value, dict):
        raise ValueError("Hermes settings must be an object")
    return value


def configure_profile() -> None:
    """Mirror the native sticky-profile selection without importing CLI/update startup."""
    from hermes_cli.profiles import resolve_profile_env
    from hermes_constants import get_default_hermes_root

    home = os.environ.get("HERMES_HOME", "").strip()
    if home and Path(home).parent.name == "profiles":
        return
    supervised = any(os.environ.get(name, "").strip().lower() in {"1", "true", "yes", "on"}
                     for name in ("HERMES_SUPERVISED_CHILD", "HERMES_S6_SUPERVISED_CHILD",
                                  "HERMES_GATEWAY_EXTERNAL_SUPERVISOR", "HERMES_GATEWAY_PROFILE_PIN"))
    name = os.environ.get("HERMES_PROFILE_NAME") or os.environ.get("HERMES_PROFILE")
    if not name and not supervised:
        try:
            name = (get_default_hermes_root() / "active_profile").read_text(encoding="utf-8-sig").strip() or "default"
        except FileNotFoundError:
            name = "default"
    if name and name != "default":
        # A missing named profile is an error, never permission to try another profile's keys.
        os.environ["HERMES_HOME"] = resolve_profile_env(name)


def configure_logging() -> None:
    from agent.redact import RedactingFormatter

    handler = logging.StreamHandler(sys.stderr)
    handler.setFormatter(RedactingFormatter("%(asctime)s [%(levelname)s] %(name)s: %(message)s"))
    root = logging.getLogger()
    root.handlers.clear()
    root.addHandler(handler)
    root.setLevel(logging.INFO)
    for name in ("httpx", "httpcore", "openai"):
        logging.getLogger(name).setLevel(logging.WARNING)


def overrides(value: dict, native: dict) -> dict:
    from hermes_cli.config import TERMINAL_CONFIG_ENV_MAP

    result = {}
    terminal = value.get("terminal") or {}
    delegation = value.get("delegation") or {}
    if not isinstance(terminal, dict) or not isinstance(delegation, dict):
        raise ValueError("terminal and delegation must be objects")
    terminal = {key: item for key, item in terminal.items() if item is not None and item != ""}
    if set(terminal) - TERMINAL_CONFIG_ENV_MAP.keys():
        raise ValueError("Unsupported Hermes terminal setting")
    if terminal.get("backend", "local") not in {"local", "docker", "ssh", "modal", "daytona", "vercel_sandbox"}:
        raise ValueError("Unsupported Hermes terminal backend")
    if terminal.get("backend", (native.get("terminal") or {}).get("backend", "local")) == "docker":
        terminal["docker_mount_cwd_to_workspace"] = True
    if terminal:
        result["terminal"] = terminal
    delegation = dict(delegation)
    routes = delegation.pop("routes", [])
    if not isinstance(routes, list) or len(routes) > 16:
        raise ValueError("Save at most 16 subagent routes")
    if routes:
        import re
        ids = set()
        for route in routes:
            if not isinstance(route, dict) or set(route) - {"id", "label", "provider", "model"}:
                raise ValueError("Invalid subagent route")
            ident = route.get("id", "")
            if not isinstance(ident, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", ident) or ident == "default" or ident in ids:
                raise ValueError("Subagent route ids must be unique")
            ids.add(ident)
            if not all(isinstance(route.get(key, ""), str) and len(route.get(key, "")) <= 512 and not any(ord(c) < 32 for c in route.get(key, "")) for key in ("label", "provider", "model")):
                raise ValueError("Invalid subagent route text")
            if not route.get("label", "").strip() or not (route.get("provider", "").strip() or route.get("model", "").strip()):
                raise ValueError("Each subagent route needs a name and a provider or model")
        delegation["backend"] = "codync-routes"
    allowed = {"provider", "model", "max_iterations", "max_concurrent_children", "child_timeout_seconds", "backend"}
    if set(delegation) - allowed:
        raise ValueError("Unsupported Hermes delegation setting")
    delegation = {key: item for key, item in delegation.items() if item is not None and item != ""}
    for key, maximum in (("max_iterations", 200), ("max_concurrent_children", 10)):
        if key in delegation and (type(delegation[key]) is not int or not 1 <= delegation[key] <= maximum):
            raise ValueError(f"{key} must be an integer between 1 and {maximum}")
    timeout = delegation.get("child_timeout_seconds", 0)
    if type(timeout) is not int or (timeout != 0 and not 30 <= timeout <= 3600):
        raise ValueError("child_timeout_seconds is an inactivity limit: use 0 or 30–3600 seconds")
    for key in ("provider", "model"):
        if key in delegation and (not isinstance(delegation[key], str) or len(delegation[key]) > 512):
            raise ValueError(f"delegation.{key} must be a short native name")
    if delegation:
        if delegation.get("provider") and delegation["provider"] != (native.get("delegation") or {}).get("provider"):
            # Resolve the selected provider through Hermes's existing credentials, never
            # pair it with an old delegation endpoint/key copied from another provider.
            delegation.update(api_key=None, base_url=None, api_mode=None, request_overrides={}, fallback_providers=[])
        result["delegation"] = delegation
    memory = value.get("memoryProvider") or "native"
    if memory not in {"native", "builtin", "honcho"}:
        raise ValueError("memoryProvider must be native, builtin or honcho")
    if memory == "honcho":
        from plugins.memory import find_provider_dir, find_provider_entry_point

        if (native.get("memory") or {}).get("provider") != "honcho":
            raise ValueError("Configure Honcho in this existing Hermes profile before selecting it in Codync")
        if find_provider_dir("honcho") is None and find_provider_entry_point("honcho") is None:
            raise ValueError("Honcho is not installed in this Hermes profile; install its native memory plugin")
    if memory != "native":
        result["memory"] = {"provider": memory}
    return result


def install_overlay(home: Path, overlay: dict):
    from hermes_cli import config, config_backend, managed_scope

    base = config_backend.get_config_backend()
    native = config.load_config()
    for section, values in overlay.items():
        for key, value in values.items():
            if base.locked(home, f"{section}.{key}") and native.get(section, {}).get(key) != value:
                raise ValueError(f"Hermes configuration locks {section}.{key}")
    # Managed scope remains authoritative. Reject a conflicting UI choice rather than presenting
    # a backend/model that the actual administrator policy will replace.
    def apply(current):
        current = config._deep_merge(current, overlay)
        delegation = overlay.get("delegation", {})
        if "request_overrides" in delegation:
            # Empty dictionaries deep-merge to the old value in Hermes. A different
            # provider needs replacement, before administrator policy is reapplied.
            current["delegation"]["request_overrides"] = copy.deepcopy(delegation["request_overrides"])
        return managed_scope.apply_managed_overlay(current) if base.honors_managed_config() else current

    effective = apply(native)
    for section, values in overlay.items():
        for key, value in values.items():
            if effective.get(section, {}).get(key) != value:
                raise ValueError(f"Hermes managed policy locks {section}.{key}")
    original_load = config.load_config
    original_readonly = config.load_config_readonly

    def merged(read):
        from hermes_constants import get_hermes_home

        current = copy.deepcopy(read())
        if get_hermes_home().resolve() != home.resolve():
            return current
        return apply(current)

    # A dedicated execution process owns exactly this profile. Install before importing
    # AIAgent, so its native consumers bind these readers. Do not change backend environment:
    # native subprocesses must still resolve the real profile's config and credentials.
    config.load_config = lambda: merged(original_load)
    config.load_config_readonly = lambda: merged(original_readonly)


def terminal_policy(value: dict, home, expected_backend=None):
    """The complete native terminal scope shared by execution and explicit health checks."""
    from hermes_cli import managed_scope
    from tools.terminal_scope import _apply_terminal_mapping, build_profile_terminal_scope

    scope = build_profile_terminal_scope(home)
    terminal = {key: item for key, item in (value.get("terminal") or {}).items() if item is not None and item != ""}
    if terminal.get("backend", scope.get("TERMINAL_ENV")) == "docker":
        terminal["docker_mount_cwd_to_workspace"] = True
    _apply_terminal_mapping(scope, terminal)
    if "docker_image" in terminal:
        scope["TERMINAL_DOCKER_IMAGE_PINNED"] = "1"
    managed_terminal = managed_scope.apply_managed_overlay({}).get("terminal")
    if isinstance(managed_terminal, dict):
        _apply_terminal_mapping(scope, managed_terminal)
        if managed_terminal.get("docker_image") is not None:
            scope["TERMINAL_DOCKER_IMAGE_PINNED"] = "1"
    if expected_backend is not None and scope.get("TERMINAL_ENV") != expected_backend:
        raise RuntimeError("Hermes terminal policy changed backend; restart this bot before executing tools")
    return scope


def construct_owner(value: dict):
    from acp_adapter.server import HermesACPAgent
    from acp_adapter.session import SessionManager
    from hermes_constants import get_hermes_home
    from tools.terminal_scope import install_and_reset_profile_terminal_scope
    from tools.delegate_tool_config import gateway_delegation_inline_scope

    home = get_hermes_home()
    quota_spec = importlib.util.spec_from_file_location("codync_hermes_delegation_quota", Path(__file__).with_name("delegation_quota.py"))
    if quota_spec is None or quota_spec.loader is None:
        raise RuntimeError("The shared subagent quota adapter is missing")
    quota = importlib.util.module_from_spec(quota_spec)
    quota_spec.loader.exec_module(quota)
    quota.install()
    routes = (value.get("delegation") or {}).get("routes") or []
    if routes:
        spec = importlib.util.spec_from_file_location("codync_hermes_delegation_routes", Path(__file__).with_name("delegation_routes.py"))
        if spec is None or spec.loader is None:
            raise RuntimeError("Saved subagent route strategy is missing")
        routing = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(routing)
        routing.install(routes)

    expected_backend = None

    def policy():
        return terminal_policy(value, home, expected_backend)

    shutdown_scope = policy()
    expected_backend = shutdown_scope.get("TERMINAL_ENV")
    cleanup_ssh = None
    if expected_backend == "ssh":
        # Hermes hardens sys.path at entry. Resolve the shipped adapter by its sibling path,
        # never by the bot's working directory or a same-named module in the installation.
        spec = importlib.util.spec_from_file_location("codync_hermes_ssh_owner", Path(__file__).with_name("ssh_owner.py"))
        if spec is None or spec.loader is None:
            raise RuntimeError("The owned SSH adapter is missing")
        adapter = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = adapter
        spec.loader.exec_module(adapter)
        cleanup_ssh = adapter.install(shutdown_scope)
        if not callable(cleanup_ssh):
            raise RuntimeError("The SSH ownership adapter did not provide its cleanup contract")

    class OwnedSessions(SessionManager):
        def _make_agent(self, **kwargs):
            with install_and_reset_profile_terminal_scope(home, prepared=policy()), gateway_delegation_inline_scope(True):
                agent = super()._make_agent(**kwargs)
                if value.get("memoryProvider") == "honcho":
                    manager = getattr(agent, "_memory_manager", None)
                    if not manager or not any(p.name == "honcho" for p in manager.providers):
                        agent.close()
                        raise RuntimeError("Honcho could not initialize; check this Hermes profile's native Honcho configuration")
                return agent

    class OwnedACP(HermesACPAgent):
        def _run_agent_turn(self, **kwargs):
            with install_and_reset_profile_terminal_scope(home, prepared=policy()), gateway_delegation_inline_scope(True):
                return super()._run_agent_turn(**kwargs)

    sessions = OwnedSessions()
    owner = OwnedACP(sessions)
    return owner, sessions, shutdown_scope, cleanup_ssh, home


async def close_owner(sessions, shutdown_scope, cleanup_ssh, home):
    from agent.interrupt_compat import request_hard_interrupt
    from tools.terminal_scope import install_and_reset_profile_terminal_scope

    finished = threading.Event()

    def attempt(label, operation):
        try:
            operation()
        except Exception:
            logging.getLogger(__name__).exception("Hermes cleanup failed during %s", label)

    def cleanup():
        try:
            with install_and_reset_profile_terminal_scope(home, prepared=shutdown_scope):
                with sessions._lock:
                    states = list(sessions._sessions.values())
                for state in states:
                    if state.cancel_event:
                        state.cancel_event.set()
                    attempt("turn interruption", lambda: request_hard_interrupt(state.agent))
                if cleanup_ssh:
                    attempt("owned SSH cleanup", cleanup_ssh)
                for state in states:
                    attempt("agent close", state.agent.close)
                from tools.terminal_tool_lifecycle import cleanup_all_environments

                attempt("environment cleanup", cleanup_all_environments)
                attempt("session end", lambda: sessions.end_all_sessions("codync_owner_closed"))
        except Exception:
            logging.getLogger(__name__).exception("Hermes owner cleanup failed")
        finally:
            finished.set()

    threading.Thread(target=cleanup, name="codync-hermes-cleanup", daemon=True).start()
    # Keep the event loop alive for native client/provider teardown. The Rust runner also owns
    # the process tree and applies its hard deadline if a native resource cannot be released.
    limit = asyncio.get_running_loop().time() + 8
    while not finished.is_set() and asyncio.get_running_loop().time() < limit:
        await asyncio.sleep(0.05)
    if not finished.is_set():
        logging.getLogger(__name__).error("Hermes cleanup exceeded eight seconds; runner termination is required")


def main():
    source_root()
    value = settings()
    import hermes_bootstrap
    hermes_bootstrap.harden_import_path()
    configure_profile()
    from agent.ssl_verify import install_truststore
    from hermes_cli.config import load_config, require_parseable_user_config
    from hermes_cli.env_loader import load_hermes_dotenv
    from hermes_constants import get_hermes_home

    configure_logging()
    load_hermes_dotenv(hermes_home=get_hermes_home())
    require_parseable_user_config()
    install_truststore()
    install_overlay(get_hermes_home(), overrides(value, load_config()))
    if sys.platform == "win32":
        from plugins.memory import import_memory_provider_module
        import_memory_provider_module()
    owner, sessions, shutdown_scope, cleanup_ssh, home = construct_owner(value)

    async def serve():
        import acp

        try:
            await acp.run_agent(owner, use_unstable_protocol=True)
        finally:
            await close_owner(sessions, shutdown_scope, cleanup_ssh, home)

    asyncio.run(serve())


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
    except Exception:
        logging.getLogger(__name__).exception("Codync Hermes owner could not start")
        raise SystemExit(1)
