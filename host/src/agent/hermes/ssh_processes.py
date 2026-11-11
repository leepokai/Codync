"""Source: Hermes tools/process_registry.py (MIT, 10d25f8a).
Adaptation: native background jobs retain a live, cancellable owned SSH handle.
"""
from __future__ import annotations

import threading


class SyncedProcess:
    """Preserve native ProcessHandle; final poll/wait fences host file delivery."""
    def __init__(self, process, project):
        self._codync_process = process
        self._project = project
        self._sync_lock = threading.RLock()
        self._pulled = False

    def __getattr__(self, name):
        return getattr(self._codync_process, name)

    def _finish(self, code):
        if code is not None and not self._pulled:
            with self._sync_lock:
                if not self._pulled:
                    try:
                        self._project.pull()
                    except Exception as error:
                        # The command already ran. Raising here makes Hermes retry it.
                        self._codync_sync_error = str(error)
                    finally:
                        self._pulled = True
        return code

    def poll(self):
        return self._finish(self._codync_process.poll())

    def wait(self, timeout=None):
        return self._finish(self._codync_process.wait(timeout=timeout))


def sync_warning(errors):
    return "\n[Command already ran; project synchronization failed: " + "; ".join(errors) + ". Do not rerun the command; resolve synchronization first.]\n"


def install(owned_class):
    from tools.process_registry import ProcessRegistry

    original_spawn = ProcessRegistry.spawn_via_env
    original_signal = ProcessRegistry._signal_kill
    original_finish = ProcessRegistry._finish_exited

    def spawn(registry, env, command, cwd=None, task_id="", session_key="", timeout=10,
              owner_task_id="", persist_on_release=False):
        if not isinstance(env, owned_class):
            return original_spawn(registry, env, command, cwd=cwd, task_id=task_id,
                                  session_key=session_key, timeout=timeout,
                                  owner_task_id=owner_task_id, persist_on_release=persist_on_release)
        if persist_on_release:
            raise ValueError("Owned SSH jobs cannot persist after the bot stops; use an explicitly managed remote service")
        process = env.spawn_owned_background(command, cwd=cwd or "")
        process._codync_environment = env
        try:
            return registry.adopt_local(process, command=command, cwd=cwd,
                                        task_id=task_id, session_key=session_key,
                                        owner_task_id=owner_task_id)
        except BaseException:
            env._kill_process(process)
            raise

    def signal(registry, session, session_id, consume_output):
        environment = getattr(session.process, "_codync_environment", None)
        if isinstance(environment, owned_class):
            environment._kill_process(session.process)
            return None
        return original_signal(registry, session, session_id, consume_output)

    def finish(registry, session, exit_code, reason="exited", source=""):
        environment = getattr(session.process, "_codync_environment", None)
        error = getattr(session.process, "_codync_sync_error", "")
        if isinstance(environment, owned_class) and error:
            registry._ingest_output(session, sync_warning([error]))
            exit_code = exit_code or 74
        return original_finish(registry, session, exit_code, reason=reason, source=source)

    ProcessRegistry.spawn_via_env = spawn
    ProcessRegistry._signal_kill = signal
    ProcessRegistry._finish_exited = finish
    closed = False

    def close():
        nonlocal closed
        if closed:
            return
        closed = True
        if ProcessRegistry.spawn_via_env is spawn:
            ProcessRegistry.spawn_via_env = original_spawn
        if ProcessRegistry._signal_kill is signal:
            ProcessRegistry._signal_kill = original_signal
        if ProcessRegistry._finish_exited is finish:
            ProcessRegistry._finish_exited = original_finish

    return close
