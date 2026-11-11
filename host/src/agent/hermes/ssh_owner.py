"""Owned SSH processes for the pinned Hermes environment backend.

Source: Hermes tools/environments/ssh.py and base_output.py (MIT, 10d25f8a).
Adaptation: private ControlMasters and acknowledged remote process-group cancellation.
"""
from __future__ import annotations

import hashlib
import importlib.util
import json
import logging
import os
import shlex
import sys
import tempfile
import threading
import time
import uuid
import weakref
from pathlib import Path

logger = logging.getLogger(__name__)

_workspace_spec = importlib.util.spec_from_file_location(
    "codync_ssh_workspace", Path(__file__).with_name("ssh_workspace.py"))
if _workspace_spec is None or _workspace_spec.loader is None:
    raise RuntimeError("The SSH project synchronization module is missing")
workspace = importlib.util.module_from_spec(_workspace_spec)
sys.modules[_workspace_spec.name] = workspace
_workspace_spec.loader.exec_module(workspace)

_process_spec = importlib.util.spec_from_file_location(
    "codync_ssh_processes", Path(__file__).with_name("ssh_processes.py"))
if _process_spec is None or _process_spec.loader is None:
    raise RuntimeError("The owned SSH process adapter is missing")
processes = importlib.util.module_from_spec(_process_spec)
sys.modules[_process_spec.name] = processes
_process_spec.loader.exec_module(processes)

# The child leader is not reaped until its process group has been killed. Its pid/pgid
# therefore cannot be reused while the controller still has authority to signal it.
# An unrelated process is never selected from a stale pid file.
REMOTE_CONTROLLER = r'''
import json, os, signal, socket, subprocess, sys, threading, time
spec = json.loads(sys.argv[1])
if not all(hasattr(os, name) for name in ("waitid", "WNOWAIT", "WEXITED")):
    raise RuntimeError("Codync SSH requires POSIX waitid(WNOWAIT)")
directory = "/tmp/cd-ssh-" + spec["nonce"]
os.mkdir(directory, 0o700)
address = directory + "/control"
server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(address)
server.listen(1)
server.settimeout(0.2)
cancelled = threading.Event()
finished = threading.Event()
lock = threading.Lock()
child = None
result = None
last_ping = time.monotonic()
def stop():
    global result
    with lock:
        if result is not None:
            return result
        if child is None:
            return 125
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        time.sleep(0.2)
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        result = child.wait(timeout=2)
        return result
def control():
    global last_ping
    while not finished.is_set():
        try:
            peer, _ = server.accept()
        except socket.timeout:
            continue
        except OSError:
            return
        try:
            with peer:
                peer.settimeout(2)
                data = bytearray()
                while len(data) <= 128 and not data.endswith(b"\n"):
                    part = peer.recv(128)
                    if not part:
                        break
                    data.extend(part)
                if bytes(data) == (spec["nonce"] + ":ping\n").encode():
                    last_ping = time.monotonic()
                    peer.sendall(b"alive\n")
                elif bytes(data) == (spec["nonce"] + ":cancel\n").encode():
                    cancelled.set()
                    stop()
                    peer.sendall(b"stopped\n")
        except OSError:
            continue
for sig in (signal.SIGHUP, signal.SIGTERM, signal.SIGINT):
    signal.signal(sig, lambda *_: cancelled.set())
worker = None
try:
    child = subprocess.Popen(["bash", "-lc" if spec["login"] else "-c", spec["command"]], start_new_session=True)
    worker = threading.Thread(target=control, daemon=True)
    worker.start()
    while not cancelled.wait(0.05):
        if time.monotonic() - last_ping >= 20:
            cancelled.set()
            break
        try:
            if os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is not None:
                break
        except ChildProcessError:
            # The authenticated control thread has already stopped and reaped it.
            break
    code = stop()
finally:
    if child is not None:
        stop()
    finished.set()
    if worker is not None:
        worker.join(timeout=3)
    server.close()
    os.unlink(address)
    os.rmdir(directory)
sys.exit(code if code >= 0 else 128 - code)
'''

REMOTE_CANCEL = r'''
import socket, sys
address, nonce = sys.argv[1:]
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as peer:
    peer.settimeout(3)
    peer.connect(address)
    peer.sendall((nonce + ":cancel\n").encode())
    with peer.makefile("rb") as reply:
        acknowledged = reply.readline(129)
    if acknowledged != b"stopped\n":
        raise RuntimeError("SSH cancellation was not acknowledged")
print("stopped")
'''

REMOTE_HEARTBEAT = r'''
import json, socket, sys
for nonce in json.loads(sys.argv[1]):
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as peer:
            peer.settimeout(1)
            peer.connect("/tmp/cd-ssh-" + nonce + "/control")
            peer.sendall((nonce + ":ping\n").encode())
            with peer.makefile("rb") as reply:
                acknowledged = reply.readline(129)
            if acknowledged != b"alive\n":
                raise RuntimeError("SSH owner heartbeat refused")
    except (FileNotFoundError, ConnectionRefusedError):
        pass  # The short command may have finished before this heartbeat arrived.
'''


def owned_ssh_class(owner: str, native_class=None, on_create=None, project_root=None):
    """Return an upstream subclass; native auth, file sync and host/key names stay intact."""
    if not isinstance(owner, str) or not owner.strip() or len(owner) > 256:
        raise ValueError("SSH owner must contain 1 to 256 characters")
    if native_class is None:
        from tools.environments.ssh import SSHEnvironment as native_class

    class OwnedSSHEnvironment(native_class):
        _codync_owned_class = True

        def __init__(self, *args, **kwargs):
            self._codync_lock = threading.RLock()
            self._codync_execution = threading.RLock()
            self._codync_commands = {}
            self._codync_control_dir = None
            self._codync_stopped = threading.Event()
            self._codync_cancel_unconfirmed = threading.Event()
            self._codync_heartbeat = None
            self._codync_closed = False
            self._codync_project = None
            self._codync_foreground = None
            self._sync_manager = None
            if project_root is not None:
                # Native home sync includes credential mounts. Only the explicit project
                # is synchronized by this owner; SSH's local identity remains untouched.
                args = list(args)
                if len(args) > 2:
                    args[2] = "~"
                else:
                    kwargs["cwd"] = "~"
                if len(args) > 7:
                    args[7] = False
                else:
                    kwargs["sync_files"] = False
            try:
                super().__init__(*args, **kwargs)
                if on_create is not None:
                    on_create(self)
            except BaseException:
                try:
                    self.cleanup()
                except Exception:
                    self._close_owned_controls()
                raise

        def init_session(self):
            if project_root is not None and self._codync_project is None:
                self._codync_project = workspace.ProjectWorkspace(self, Path(project_root), owner)
                self._codync_project.push()
                self.cwd = str(self._codync_project.remote)
                # Reuse native file-tools path translation (including Windows
                # drive paths) rather than rewriting arbitrary shell programs.
                self.host_cwd = str(self._codync_project.local)
                self.host_cwd_mount = self.cwd
            super().init_session()

        def _before_execute(self):
            if self._codync_project is not None:
                self._codync_project.push()
            super()._before_execute()

        def spawn_owned_background(self, command, cwd=""):
            from tools.environments.base_session_env import _passthrough_save_restore

            with self._codync_execution:
                self._before_execute()
                cwd = self._codync_project.cwd(cwd or self.cwd) if self._codync_project is not None else (cwd or self.cwd)
                command, stdin = self._prepare_command(command)
                save, restore = _passthrough_save_restore(self._snapshot_excluded_passthrough_names())
                parts = list(save)
                if self._snapshot_ready:
                    parts.append(f"source {self._quote_shell_path(self._snapshot_path)} >/dev/null 2>&1 || true")
                parts += restore
                parts += ['export AI_AGENT="${AI_AGENT:-hermes-agent}" HERMES_AGENT="${HERMES_AGENT:-true}"',
                          'export GIT_PAGER="${GIT_PAGER:-cat}" PAGER="${PAGER:-cat}"',
                          f"builtin cd -- {self._quote_cwd_for_cd(cwd)} || exit 126", command]
                return self._run_bash("\n".join(parts), login=not self._snapshot_ready and not self._prefer_nonlogin,
                                      stdin_data=stdin)

        def execute(self, command, cwd="", **kwargs):
            with self._codync_execution:
                if self._codync_project is not None:
                    cwd = self._codync_project.cwd(cwd or self.cwd)
                completed = []
                self._codync_foreground = completed
                try:
                    result = super().execute(command, cwd=cwd, **kwargs)
                finally:
                    self._codync_foreground = None
                errors = [getattr(process, "_codync_sync_error", "") for process in completed]
                errors = [error for error in errors if error]
                if errors:
                    result["codync_sync_error"] = "; ".join(errors)
                    result["returncode"] = result.get("returncode") or 74
                    result["output"] = result.get("output", "") + processes.sync_warning(errors)
                if self._codync_project is not None:
                    result["codync_workspace"] = {"host": str(self._codync_project.local),
                                                   "remote": str(self._codync_project.remote)}
                return result

        def _establish_connection(self):
            if self._codync_control_dir is None:
                # A private directory also fences upstream's prefix-glob cleanup. /tmp keeps
                # Unix socket paths below the macOS limit even under a long launch TMPDIR.
                base = None if os.name == "nt" else "/tmp"
                self._codync_control_dir = Path(tempfile.mkdtemp(prefix="cd-ssh-", dir=base))
                digest = hashlib.sha256((owner + ":" + uuid.uuid4().hex).encode()).hexdigest()[:16]
                self.control_dir = self._codync_control_dir
                self.control_socket = self.control_dir / (digest + ".sock")
            super()._establish_connection()
            check = "import os,socket; assert hasattr(os,'WNOWAIT') and hasattr(os,'waitid') and hasattr(socket,'AF_UNIX')"
            result = self._run_ssh(shlex.join(["python3", "-c", check]), timeout=15)
            if result.returncode != 0:
                raise RuntimeError("Codync SSH needs remote Python 3 with POSIX waitid(WNOWAIT); no unsafe fallback was used")

        def _run_bash(self, cmd_string, *, login=False, timeout=120, stdin_data=None):
            from tools.environments.base_output import _popen_bash
            from tools.environments.remote_common import client_env_with, prepend_unset, resolve_passthrough_env
            from tools.environments.ssh import _load_hermes_env_vars

            if self._codync_stopped.is_set():
                raise RuntimeError("Owned SSH environment is closed")
            if self._codync_project is not None:
                self._codync_project.before_spawn()
            values, unset_names = resolve_passthrough_env(hermes_env_loader=_load_hermes_env_vars)
            nonce = uuid.uuid4().hex
            spec = {"command": prepend_unset(cmd_string, unset_names), "login": bool(login), "nonce": nonce}
            remote = shlex.join(["python3", "-c", REMOTE_CONTROLLER, json.dumps(spec)])
            command = self._build_ssh_command(send_env=values) + [remote]
            client_env = client_env_with(values)
            process = _popen_bash(command, stdin_data, **({"env": client_env} if client_env is not None else {}))
            with self._codync_lock:
                self._codync_commands = {p: token for p, token in self._codync_commands.items() if p.poll() is None}
                self._codync_commands[process] = nonce
                if self._codync_heartbeat is None:
                    self._codync_heartbeat = threading.Thread(target=self._heartbeat, name="codync-ssh-owner", daemon=True)
                    self._codync_heartbeat.start()
            wrapped = processes.SyncedProcess(process, self._codync_project) if self._codync_project is not None else process
            if self._codync_foreground is not None:
                self._codync_foreground.append(wrapped)
            return wrapped

        def _heartbeat(self):
            while not self._codync_stopped.wait(2):
                with self._codync_lock:
                    active = [nonce for process, nonce in self._codync_commands.items() if process.poll() is None]
                if not active:
                    continue
                command = shlex.join(["python3", "-c", REMOTE_HEARTBEAT, json.dumps(active)])
                try:
                    result = self._run_ssh(command, timeout=4)
                    if result.returncode != 0:
                        logger.warning("SSH owner heartbeat failed; remote work will stop when its lease expires")
                except Exception:
                    logger.warning("SSH owner heartbeat unavailable; remote work will stop when its lease expires")

        def _kill_process(self, process):
            process = getattr(process, "_codync_process", process)
            with self._codync_lock:
                nonce = self._codync_commands.get(process)
            if nonce is not None and process.poll() is None:
                command = shlex.join(["python3", "-c", REMOTE_CANCEL, "/tmp/cd-ssh-" + nonce + "/control", nonce])
                try:
                    result = self._run_ssh(command, timeout=4)
                    if result.returncode != 0 or result.stdout.strip() != "stopped":
                        self._codync_cancel_unconfirmed.set()
                        logger.warning("SSH remote cancellation was not acknowledged; closing the owned transport")
                except Exception:
                    self._codync_cancel_unconfirmed.set()
                    logger.warning("SSH cancellation transport failed; closing the owned transport")
            # The controller also treats the SSH session's HUP/TERM as cancellation.
            super()._kill_process(process)
            with self._codync_lock:
                self._codync_commands.pop(process, None)

        def _force_kill_process(self, process):
            # Never wait on a second SSH connection during the native hard-exit fence.
            # Its remote controller's bounded lease covers transports that do not deliver HUP.
            super()._kill_process(getattr(process, "_codync_process", process))

        def _close_owned_controls(self):
            directory = self._codync_control_dir
            if directory is None:
                return
            try:
                # Never sync a partially constructed environment; only its own private masters.
                import subprocess
                for socket in directory.glob("*.sock"):
                    subprocess.run(["ssh", "-o", "ControlPath=" + str(socket), "-O", "exit", f"{self.user}@{self.host}"],
                                   stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                   timeout=5, check=False)
                    socket.unlink(missing_ok=True)
                directory.rmdir()
            except Exception:
                logger.warning("Owned SSH control directory could not be fully released")

        def cleanup(self):
            with self._codync_lock:
                if self._codync_closed:
                    return
                self._codync_closed = True
            self._codync_stopped.set()
            with self._codync_lock:
                running = list(self._codync_commands)
            # Dispatch every cancellation first. A dead host must not consume one
            # full SSH timeout for each job before the next job receives Stop.
            cancelling = [threading.Thread(target=self._kill_process, args=(process,), daemon=True)
                          for process in running]
            for worker in cancelling:
                worker.start()
            deadline = time.monotonic() + 4.5
            for worker in cancelling:
                worker.join(timeout=max(0, deadline - time.monotonic()))
            try:
                # Completed tools have already synchronized. Shutdown never races
                # a download against an unacknowledged still-writing command.
                if (self._codync_project is not None and not self._codync_cancel_unconfirmed.is_set()
                        and not any(worker.is_alive() for worker in cancelling)):
                    self._codync_project.pull(timeout=1.5)
            finally:
                try:
                    super().cleanup()
                finally:
                    self._close_owned_controls()

    return OwnedSSHEnvironment


def install(terminal_scope: dict[str, str]):
    """Install an owned SSH factory and return its idempotent shutdown callback.

    Other backends are untouched. The caller owns the complete prepared terminal policy;
    call both this function and its cleanup while that policy is bound to the context.
    """
    if terminal_scope.get("TERMINAL_ENV", "local") != "ssh":
        return lambda: None
    from tools import terminal_tool_backends

    project_text = os.environ.get("CODYNC_HERMES_PROJECT_ROOT", "").strip()
    if not project_text:
        raise RuntimeError("The host must supply this bot's project root for SSH synchronization")
    project_root = workspace.validate_project_root(project_text)

    original = terminal_tool_backends._SSHEnvironment
    if getattr(original, "_codync_owned_class", False):
        raise RuntimeError("An SSH execution owner is already installed in this interpreter")
    instances = weakref.WeakSet()
    lock = threading.RLock()
    closed = False

    def track(environment):
        with lock:
            if closed:
                environment.cleanup()
                raise RuntimeError("Owned SSH runtime is closing")
            instances.add(environment)

    implementation = owned_ssh_class("codync-" + str(os.getpid()) + "-" + uuid.uuid4().hex,
                                     native_class=original, on_create=track, project_root=project_root)
    terminal_tool_backends._SSHEnvironment = implementation
    try:
        close_processes = processes.install(implementation)
    except BaseException:
        terminal_tool_backends._SSHEnvironment = original
        raise

    def close():
        nonlocal closed
        with lock:
            if closed:
                return
            closed = True
            pending = list(instances)
            instances.clear()
        first_error = None
        try:
            def finish(environment):
                nonlocal first_error
                try:
                    environment.cleanup()
                except Exception as error:
                    with lock:
                        if first_error is None:
                            first_error = error
            workers = [threading.Thread(target=finish, args=(environment,), daemon=True) for environment in pending]
            for worker in workers:
                worker.start()
            deadline = time.monotonic() + 7
            for worker in workers:
                worker.join(timeout=max(0, deadline - time.monotonic()))
            if any(worker.is_alive() for worker in workers):
                first_error = RuntimeError("SSH shutdown exceeded its grace; the remote owner lease will stop remaining jobs")
        finally:
            close_processes()
            if terminal_tool_backends._SSHEnvironment is implementation:
                terminal_tool_backends._SSHEnvironment = original
        if first_error is not None:
            raise first_error

    return close
