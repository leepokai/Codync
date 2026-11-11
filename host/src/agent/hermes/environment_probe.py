"""Source: Hermes agent/prompt_builder.py::_run_backend_probe (MIT, 10d25f8a).
Adaptation: explicit, bounded, inference-free health check of the saved native policy.
"""
from __future__ import annotations

import contextvars
import importlib
import importlib.util
import json
import logging
import os
from pathlib import Path
import sys
import threading
import time
import uuid

from acp_owner import configure_logging, configure_profile, install_overlay, overrides, settings, source_root, terminal_policy

BACKENDS = {"local", "docker", "ssh", "modal", "daytona", "vercel_sandbox"}
UNAVAILABLE = "Saved environment could not be checked. Verify its existing connection, profile, dependencies and terminal policy."
UNCONFIRMED = "Environment check did not finish or probe cleanup could not be confirmed. Check the native sandbox dashboard."


def build_probe(backend, config, task_id):
    """Use the same native factory/config shapers, with only private lifecycle changes."""
    from tools.terminal_tool_backends import (
        _container_config_from_config, _create_environment, _local_config_from_config, _ssh_config_from_config,
    )
    from tools.terminal_tool_config import _is_container_backend

    container = _container_config_from_config(config) if _is_container_backend(backend) else None
    if container is not None:
        # A health check must never attach to a running bot or reap its resources.
        # Network, volumes, image, user, resource limits and credentials remain native.
        container.update(container_persistent=False, docker_persist_across_processes=False,
                         docker_shared_container_key="", docker_orphan_reaper=False)
    image = config.get(f"{backend}_image", "") if backend in {"docker", "modal", "daytona"} else ""
    return _create_environment(
        env_type=backend, image=image, cwd=config.get("cwd", ""), timeout=5,
        ssh_config=_ssh_config_from_config(config) if backend == "ssh" else None,
        container_config=container, local_config=_local_config_from_config(config) if backend == "local" else None,
        task_id=task_id, host_cwd=config.get("host_cwd"), probe_only=True,
    )


def track_constructor(module, name, environments):
    original = getattr(module, name)
    class ProbeOwned(original):
        def __init__(self, *args, **kwargs):
            environments.append(self)  # retain partial construction for best-effort teardown
            super().__init__(*args, **kwargs)
    setattr(module, name, ProbeOwned)


def track_backend(backend, environments):
    from tools import terminal_tool_backends as builders

    names = {"local": ("_LocalEnvironment",), "docker": ("_DockerEnvironment",),
             "ssh": ("_SSHEnvironment",), "modal": ("_ModalEnvironment", "_ManagedModalEnvironment")}
    for name in names.get(backend, ()):
        track_constructor(builders, name, environments)
    if backend in {"daytona", "vercel_sandbox"}:
        module = importlib.import_module("tools.environments." + backend)
        track_constructor(module, "DaytonaEnvironment" if backend == "daytona" else "VercelSandboxEnvironment", environments)
    if backend in {"modal", "daytona", "vercel_sandbox"}:
        # A constant readiness command does not need Hermes home/credential file copies.
        # SDK access still uses the selected native profile's existing authentication.
        module = importlib.import_module("tools.environments." + backend)
        module.iter_sync_files = lambda *_args, **_kwargs: []


def cleanup_probe(environments):
    from tools.terminal_tool_lifecycle import _cleanup_env

    failures = []
    class Warnings(logging.Handler):
        def emit(self, record):
            if record.levelno >= logging.WARNING:
                failures.append(True)
    handler = Warnings()
    logging.getLogger().addHandler(handler)
    try:
        for environment in reversed(environments):
            try:
                _cleanup_env(environment, force_remove=True)
                wait = getattr(environment, "wait_for_cleanup", None)
                if callable(wait) and wait(timeout=12) is False:
                    failures.append(True)
            except Exception:
                failures.append(True)
    finally:
        logging.getLogger().removeHandler(handler)
    return not failures


def run_probe(value, home, scope, budget):
    """No ACP session/model is created. Native execution is a fixed readiness token."""
    from tools.terminal_scope import install_and_reset_profile_terminal_scope

    backend = scope.get("TERMINAL_ENV", "local")
    if backend not in BACKENDS:
        raise ValueError("Unknown saved environment")
    close_ssh = None
    if backend == "ssh":
        spec = importlib.util.spec_from_file_location("codync_health_ssh", Path(__file__).with_name("ssh_owner.py"))
        if spec is None or spec.loader is None:
            raise RuntimeError("Owned SSH adapter is missing")
        adapter = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = adapter
        spec.loader.exec_module(adapter)
        close_ssh = adapter.install(scope)
    environments = []
    finished = threading.Event()
    aborting = threading.Event()
    outcome = {"backend": backend, "status": "unavailable", "detail": UNCONFIRMED}

    def work():
        cleanup_ok = False
        try:
            with install_and_reset_profile_terminal_scope(home, prepared=scope):
                from tools import terminal_tool
                track_backend(backend, environments)
                config = terminal_tool._get_env_config()
                project = os.environ.get("CODYNC_HERMES_PROJECT_ROOT", "")
                if not project or not Path(project).is_dir():
                    raise ValueError("The bot project directory is unavailable")
                if backend == "local":
                    config["cwd"] = project
                if backend == "docker":
                    config["host_cwd"] = project
                task = "codync-health-" + uuid.uuid4().hex
                environment = build_probe(backend, config, task)
                if aborting.is_set():
                    return
                token = "codync-health-" + uuid.uuid4().hex
                result = environment.execute("printf '%s\\n' '" + token + "'", timeout=5)
                healthy = result.get("returncode") == 0 and result.get("output", "").strip() == token
                outcome.update(status="healthy" if healthy else "unavailable",
                               detail="Saved environment executed its readiness command and probe cleanup completed." if healthy else UNAVAILABLE)
        except Exception:
            outcome.update(status="unavailable", detail=UNAVAILABLE)
        finally:
            with install_and_reset_profile_terminal_scope(home, prepared=scope):
                cleanup_ok = cleanup_probe(environments)
                if close_ssh:
                    try:
                        close_ssh()
                    except Exception:
                        cleanup_ok = False
            if not cleanup_ok:
                outcome.update(status="unavailable", detail=UNCONFIRMED)
            finished.set()

    # Native SDK startup/teardown can wedge. The outer host owns this process tree
    # and its hard deadline; a timed-out cloud cleanup is never reported healthy.
    context = contextvars.copy_context()
    threading.Thread(target=lambda: context.run(work), daemon=True, name="codync-environment-health").start()
    started = time.monotonic()
    if not finished.wait(max(0.1, budget - 12)):
        aborting.set()
        def cancel():
            with install_and_reset_profile_terminal_scope(home, prepared=scope):
                if close_ssh:
                    try:
                        close_ssh()
                    except Exception:
                        pass  # the result already declares cleanup unconfirmed
                cleanup_probe(list(environments))
        threading.Thread(target=cancel, daemon=True, name="codync-health-timeout-cleanup").start()
        finished.wait(max(0, budget - (time.monotonic() - started)))
        return {"backend": backend, "status": "unavailable", "detail": UNCONFIRMED}
    return outcome


def main():
    source_root()
    import hermes_bootstrap
    hermes_bootstrap.harden_import_path()
    configure_profile()
    from hermes_constants import get_hermes_home
    from hermes_cli.config import load_config, require_parseable_user_config
    from hermes_cli.env_loader import load_hermes_dotenv
    from agent.ssl_verify import install_truststore

    configure_logging()
    value = settings()
    load_hermes_dotenv(hermes_home=get_hermes_home())
    require_parseable_user_config()
    install_truststore()
    install_overlay(get_hermes_home(), overrides(value, load_config()))
    scope = terminal_policy(value, get_hermes_home())
    budget = min(50.0, max(1.0, float(os.environ.get("CODYNC_HERMES_HEALTH_BUDGET", "50"))))
    return run_probe(value, get_hermes_home(), scope, budget)


if __name__ == "__main__":
    result_stream = sys.stdout
    sys.stdout = sys.stderr
    try:
        # Native SDK output cannot corrupt the structured result or reveal credentials.
        reply = main()
    except Exception:
        reply = {"backend": "unknown", "status": "unavailable", "detail": UNAVAILABLE}
    result_stream.write(json.dumps(reply) + "\n")
    result_stream.flush()
    # SDK helper threads must not keep a finished or timed-out check alive.
    # The Rust Process owner reaps only this probe's descendants after its leader exits.
    os._exit(0)
