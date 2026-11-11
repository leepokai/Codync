"""Owned SSH regressions. No network or real SSH server is required."""
from __future__ import annotations

import importlib.util
import io
import json
import os
import shlex
import socket
import subprocess
import sys
import tempfile
import tarfile
import threading
import time
import unittest
import uuid
from unittest import mock
from pathlib import Path, PurePosixPath
from types import SimpleNamespace

_spec = importlib.util.spec_from_file_location("codync_ssh_owner", Path(__file__).with_name("ssh_owner.py"))
ssh_owner = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ssh_owner)


class NativeFixture:
    def __init__(self, host, user, **options):
        self.host, self.user, self.options = host, user, options
        self.native_cleanup = False
        self._establish_connection()

    def _establish_connection(self):
        self.connected = True

    def _run_ssh(self, command, timeout):
        return SimpleNamespace(returncode=0, stdout="ready")

    def cleanup(self):
        self.native_cleanup = True

    def _kill_process(self, process):
        process.kill()


class OwnershipTests(unittest.TestCase):
    def test_foreground_sync_failure_returns_error_without_reexecuting_command(self):
        class NativeExecute(NativeFixture):
            def execute(self, command, **kwargs):
                self.calls = getattr(self, "calls", 0) + 1
                process = ssh_owner.processes.SyncedProcess(SimpleNamespace(poll=lambda: 0), self._codync_project)
                self._codync_foreground.append(process)
                process.poll()
                return {"output": "written", "returncode": 0}
        environment = ssh_owner.owned_ssh_class("bot", NativeExecute)("host", "user")
        self.addCleanup(environment.cleanup)
        project = mock.Mock(local="C:/project", remote="/remote")
        project.cwd.return_value = "/remote"
        project.pull.side_effect = RuntimeError("sync conflict")
        environment._codync_project = project
        result = environment.execute("write artifact", cwd="/remote")
        self.assertEqual(environment.calls, 1)
        self.assertEqual(result["returncode"], 74)
        self.assertIn("Command already ran", result["output"])
        self.assertEqual(result["codync_sync_error"], "sync conflict")
        environment._codync_project = None

    def test_stop_dispatches_all_remote_cancellations_before_waiting_for_any(self):
        environment = ssh_owner.owned_ssh_class("bot", NativeFixture)("host", "user")
        started = []
        failures = []
        all_started = threading.Event()
        jobs = [mock.Mock(), mock.Mock(), mock.Mock()]
        environment._codync_commands = dict.fromkeys(jobs, "nonce")
        def cancel(process):
            started.append(process)
            if len(started) == len(jobs):
                all_started.set()
            if not all_started.wait(1):
                failures.append(process)
        environment._kill_process = cancel
        environment.cleanup()
        self.assertCountEqual(started, jobs)
        self.assertEqual(failures, [], "Stop must not serialize network cancellation")

    def test_install_closes_its_instances_and_restores_only_its_own_factory(self):
        backend = SimpleNamespace(_SSHEnvironment=NativeFixture)
        tools = SimpleNamespace(terminal_tool_backends=backend)
        with tempfile.TemporaryDirectory() as project, mock.patch.dict(os.environ, {"CODYNC_HERMES_PROJECT_ROOT": project}), mock.patch.dict(sys.modules, {"tools": tools}), mock.patch.object(ssh_owner.processes, "install", return_value=lambda: None):
            close = ssh_owner.install({"TERMINAL_ENV": "ssh"})
            self.assertIsNot(backend._SSHEnvironment, NativeFixture)
            environment = backend._SSHEnvironment("existing-host", "existing-user")
            close()
            close()
            self.assertTrue(environment.native_cleanup)
            self.assertFalse(environment.options["sync_files"], "native credential mounts must stay on the host")
            self.assertFalse(environment.control_dir.exists())
            self.assertIs(backend._SSHEnvironment, NativeFixture)

    def test_non_ssh_install_does_not_import_or_modify_the_native_registry(self):
        backend = SimpleNamespace(_SSHEnvironment=NativeFixture)
        tools = SimpleNamespace(terminal_tool_backends=backend)
        with mock.patch.dict(sys.modules, {"tools": tools}):
            ssh_owner.install({"TERMINAL_ENV": "docker"})()
            self.assertIs(backend._SSHEnvironment, NativeFixture)

    def test_each_instance_gets_private_controls_without_changing_auth_or_sync_options(self):
        implementation = ssh_owner.owned_ssh_class("bot-one", NativeFixture)
        options = {"port": 2222, "key_path": "existing-key", "sync_files": True, "persistent": True}
        first = implementation("existing-host", "existing-user", **options)
        second = implementation("existing-host", "existing-user", **options)
        try:
            self.assertEqual(first.options, options)
            self.assertEqual(first.host, "existing-host")
            self.assertEqual(first.user, "existing-user")
            self.assertNotEqual(first.control_socket, second.control_socket)
            self.assertNotEqual(first.control_dir, second.control_dir)
            first.cleanup()
            self.assertTrue(first.native_cleanup)
            self.assertFalse(first.control_dir.exists())
            self.assertTrue(second.control_dir.exists(), "another owner must keep its controls")
        finally:
            first.cleanup()
            second.cleanup()

    def test_owner_identity_is_required(self):
        for owner in (None, "", " ", "a" * 257):
            with self.assertRaises(ValueError):
                ssh_owner.owned_ssh_class(owner, NativeFixture)

    def test_ssh_install_requires_an_explicit_host_project(self):
        backend = SimpleNamespace(_SSHEnvironment=NativeFixture)
        with mock.patch.dict(sys.modules, {"tools": SimpleNamespace(terminal_tool_backends=backend)}), mock.patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(RuntimeError, "project root"):
                ssh_owner.install({"TERMINAL_ENV": "ssh"})
        self.assertIs(backend._SSHEnvironment, NativeFixture)


class WorkspaceTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.project = ssh_owner.workspace.ProjectWorkspace.__new__(ssh_owner.workspace.ProjectWorkspace)
        self.project.local = Path(self.directory.name)
        self.project.remote = PurePosixPath("/home/remote/.codync-workspaces/bot/session")
        self.project.lock = threading.RLock()
        self.project.manager = SimpleNamespace(_pushed_hashes={}, _synced_files={})

    def archive(self, entries):
        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode="w") as output:
            for name, content in entries:
                info = tarfile.TarInfo(name)
                if content is None:
                    info.type = tarfile.SYMTYPE
                    info.linkname = "/outside"
                    output.addfile(info)
                else:
                    info.size = len(content)
                    output.addfile(info, io.BytesIO(content))
        self.project._download = lambda destination: Path(destination).write_bytes(raw.getvalue())

    def baseline(self, name, contents):
        path = self.project.local / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(contents)
        remote = str(self.project.remote / name)
        sha = ssh_owner.workspace.digest(path)
        self.project.manager._pushed_hashes[remote] = sha
        self.project.manager._synced_files[remote] = (1, 2, 3, 4)
        return path, remote, sha

    def test_project_sources_exclude_credentials_caches_and_links(self):
        for name in ["app.py", ".env", ".env.production", "id_rsa", "private.pem", "auth.json", "credentials/key.txt", ".git/config", ".config/provider.json", "node_modules/code.js"]:
            path = self.project.local / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("test")
        files = self.project.files()
        self.assertEqual([Path(local).name for local, _ in files], ["app.py"])

    def test_host_cwd_maps_to_project_without_parent_escape(self):
        sub = self.project.local / "sub"
        sub.mkdir()
        self.assertEqual(self.project.cwd(str(sub)), str(self.project.remote / "sub"))
        self.assertEqual(self.project.cwd("~"), str(self.project.remote))
        self.assertEqual(self.project.cwd(str(self.project.remote / "sub")), str(self.project.remote / "sub"))
        for path in [str(self.project.local.parent), str(self.project.remote / ".." / "outside")]:
            with self.assertRaises(ValueError):
                self.project.cwd(path)

    def test_new_remote_outputs_are_available_to_host_file_tools_after_pull(self):
        self.archive([("results/report.txt", b"written remotely")])
        self.project.pull()
        self.assertEqual((self.project.local / "results/report.txt").read_bytes(), b"written remotely")
        remote = str(self.project.remote / "results/report.txt")
        self.assertEqual(self.project.manager._synced_files[remote], (-1, -1, -1, -1))
        self.assertEqual(self.project.manager._pushed_hashes[remote], ssh_owner.workspace.digest(self.project.local / "results/report.txt"))

    def test_local_remote_conflict_is_visible_and_does_not_overwrite_host_work(self):
        path, _, _ = self.baseline("app.py", b"original")
        path.write_bytes(b"local edit")
        self.archive([("app.py", b"remote edit")])
        with self.assertRaisesRegex(RuntimeError, "both changed"):
            self.project.pull()
        self.assertEqual(path.read_bytes(), b"local edit")

    def test_local_edit_after_download_preflight_is_preserved_before_replacement(self):
        path, _, _ = self.baseline("app.py", b"original")
        self.archive([("app.py", b"remote edit")])
        original = os.chmod
        def chmod(destination, mode, *args, **kwargs):
            original(destination, mode, *args, **kwargs)
            if Path(destination).name.startswith(".codync-sync-"):
                path.write_bytes(b"edited during staging")
        with mock.patch.object(ssh_owner.workspace.os, "chmod", side_effect=chmod):
            with self.assertRaisesRegex(RuntimeError, "changed during download"):
                self.project.pull()
        self.assertEqual(path.read_bytes(), b"edited during staging")

    def test_local_edit_after_delete_preflight_is_preserved_before_unlink(self):
        path, _, _ = self.baseline("app.py", b"original")
        self.archive([])
        original = self.project._safe_target
        checks = []
        def check(destination):
            original(destination)
            checks.append(destination)
            if len(checks) == 3:
                path.write_bytes(b"edited before delete")
        with mock.patch.object(self.project, "_safe_target", side_effect=check):
            with self.assertRaisesRegex(RuntimeError, "changed during download"):
                self.project.pull()
        self.assertEqual(path.read_bytes(), b"edited before delete")

    def test_host_only_edit_is_not_acknowledged_as_already_uploaded(self):
        path, remote, baseline = self.baseline("app.py", b"original")
        path.write_bytes(b"local edit")
        self.archive([("app.py", b"original")])
        self.project.pull()
        self.assertEqual(path.read_bytes(), b"local edit")
        self.assertEqual(self.project.manager._pushed_hashes[remote], baseline)
        self.assertEqual(self.project.manager._synced_files[remote], (1, 2, 3, 4))

    def test_unmodified_local_file_follows_remote_deletion(self):
        path, remote, _ = self.baseline("app.py", b"original")
        self.archive([])
        self.project.pull()
        self.assertFalse(path.exists())
        self.assertNotIn(remote, self.project.manager._pushed_hashes)

    def test_archive_validation_finishes_before_writing_any_host_file(self):
        for unsafe in ["../outside", "/absolute", ".env", "secret.key", "a\\b", "CON"]:
            with self.subTest(path=unsafe):
                self.archive([("safe.txt", b"safe"), (unsafe, b"bad")])
                with self.assertRaises(RuntimeError):
                    self.project.pull()
                self.assertFalse((self.project.local / "safe.txt").exists())
        for entries in [[("link", None)], [("same", b"one"), ("SAME", b"two")], [("parent", b"file"), ("parent/child", b"file")]]:
            self.archive(entries)
            with self.assertRaises(RuntimeError):
                self.project.pull()

    def test_transfer_failure_cannot_be_hidden_by_native_best_effort_sync(self):
        self.baseline("app.py", b"not uploaded")
        self.project.pull = lambda: None
        self.project.manager = SimpleNamespace(sync=lambda **_: None, _pushed_hashes={})
        with self.assertRaisesRegex(RuntimeError, "synchronization failed"):
            self.project.push()

    def test_final_process_poll_pulls_files_once_before_returning_completion(self):
        project = mock.Mock()
        process = SimpleNamespace(poll=lambda: 0, wait=lambda timeout=None: 0, pid=42)
        wrapped = ssh_owner.processes.SyncedProcess(process, project)
        self.assertEqual(wrapped.poll(), 0)
        self.assertEqual(wrapped.wait(), 0)
        project.pull.assert_called_once_with()
        self.assertEqual(wrapped.pid, 42)

    def test_sync_failure_after_command_exit_is_recorded_without_triggering_native_retry(self):
        project = mock.Mock()
        project.pull.side_effect = RuntimeError("conflicting host edit")
        process = SimpleNamespace(poll=lambda: 0, wait=lambda timeout=None: 0)
        wrapped = ssh_owner.processes.SyncedProcess(process, project)
        self.assertEqual(wrapped.poll(), 0)
        self.assertIn("conflicting host edit", wrapped._codync_sync_error)
        self.assertEqual(wrapped.wait(), 0)
        project.pull.assert_called_once_with()

    def test_an_unreturned_remote_change_is_pulled_before_any_new_upload(self):
        self.project.manager = SimpleNamespace(sync=mock.Mock(), _pushed_hashes={})
        self.project.pull = mock.Mock(side_effect=RuntimeError("sync conflict"))
        with self.assertRaisesRegex(RuntimeError, "sync conflict"):
            self.project.push()
        self.project.manager.sync.assert_not_called()

    def test_swapped_symlink_sources_and_hardlinks_are_refused(self):
        with tempfile.TemporaryDirectory() as outside:
            target = Path(outside) / "secret.txt"
            target.write_text("must not upload")
            link = self.project.local / "looks-like-source.txt"
            try:
                os.symlink(target, link)
            except OSError:
                self.skipTest("creating symlinks requires platform permission")
            with self.assertRaises(RuntimeError):
                self.project._open_source(link)
            link.unlink()
            try:
                os.link(target, link)
            except OSError:
                self.skipTest("hardlinks are unavailable on this filesystem")
            with self.assertRaises(RuntimeError):
                self.project._open_source(link)


class ProcessAdapterTests(unittest.TestCase):
    def setUp(self):
        self.native_spawn = mock.Mock(return_value="other-backend")
        self.native_signal = mock.Mock(return_value="other-signal")
        self.native_finish = mock.Mock()
        self.ingest = mock.Mock()
        class Registry:
            spawn_via_env = self.native_spawn
            _signal_kill = self.native_signal
            _finish_exited = self.native_finish
            _ingest_output = self.ingest
            def adopt_local(self, process, **kwargs):
                return SimpleNamespace(process=process, metadata=kwargs)
        class Owned:
            def __init__(self):
                self.started = []
                self.killed = []
            def spawn_owned_background(self, command, cwd=""):
                self.started.append((command, cwd))
                return SimpleNamespace(pid=42)
            def _kill_process(self, process):
                self.killed.append(process)
        self.Registry = Registry
        self.Owned = Owned

    def test_background_logs_and_kills_keep_the_live_owned_handle(self):
        with mock.patch.dict(sys.modules, {"tools.process_registry": SimpleNamespace(ProcessRegistry=self.Registry)}):
            close = ssh_owner.processes.install(self.Owned)
            self.addCleanup(close)
            registry = self.Registry()
            environment = self.Owned()
            session = registry.spawn_via_env(environment, "serve", cwd="/project", task_id="task")
            self.assertEqual(environment.started, [("serve", "/project")])
            self.assertIs(session.process._codync_environment, environment)
            registry._signal_kill(session, "id", True)
            self.assertEqual(environment.killed, [session.process])
            self.native_signal.assert_not_called()
            self.assertEqual(registry.spawn_via_env(object(), "other"), "other-backend")
            close()
            close()
            self.assertIs(self.Registry.spawn_via_env, self.native_spawn)

    def test_explicit_detach_cannot_leave_an_unowned_remote_job(self):
        with mock.patch.dict(sys.modules, {"tools.process_registry": SimpleNamespace(ProcessRegistry=self.Registry)}):
            close = ssh_owner.processes.install(self.Owned)
            self.addCleanup(close)
            environment = self.Owned()
            with self.assertRaisesRegex(ValueError, "cannot persist"):
                self.Registry().spawn_via_env(environment, "serve", persist_on_release=True)
            self.assertEqual(environment.started, [])

    def test_background_sync_failure_is_reported_as_an_error_after_command_completion(self):
        with mock.patch.dict(sys.modules, {"tools.process_registry": SimpleNamespace(ProcessRegistry=self.Registry)}):
            close = ssh_owner.processes.install(self.Owned)
            self.addCleanup(close)
            registry = self.Registry()
            session = registry.spawn_via_env(self.Owned(), "generate report")
            session.process._codync_sync_error = "conflict"
            registry._finish_exited(session, 0)
            self.native_finish.assert_called_once_with(session, 74, reason="exited", source="")
            self.assertIn("Command already ran", self.ingest.call_args.args[1])


@unittest.skipUnless(os.name == "posix" and hasattr(os, "WNOWAIT"), "remote controller requires POSIX waitid")
class ControllerTests(unittest.TestCase):
    def start_controller(self, command, *, piped=False):
        nonce = uuid.uuid4().hex
        args = [sys.executable, "-c", ssh_owner.REMOTE_CONTROLLER,
                json.dumps({"nonce": nonce, "login": False, "command": command})]
        process = subprocess.Popen(args, stdin=subprocess.PIPE if piped else subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(self.kill_controller, process)
        return process, nonce

    @staticmethod
    def kill_controller(process):
        if process.poll() is None:
            process.terminate()
            try:
                process.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.communicate(timeout=5)

    @staticmethod
    def wait_until(predicate, timeout=5):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            time.sleep(0.01)
        raise AssertionError("controller condition did not complete")

    def test_an_authenticated_cancel_is_acknowledged_after_its_child_is_stopped(self):
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / "pid"
            command = "printf '%s' \"$$\" > " + shlex.quote(str(pid_file)) + "; sleep 60 & wait"
            process, nonce = self.start_controller(command)
            address = "/tmp/cd-ssh-" + nonce + "/control"
            self.wait_until(lambda: pid_file.exists() and Path(address).exists())
            child = int(pid_file.read_text())
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as peer:
                peer.settimeout(5)
                peer.connect(address)
                peer.sendall((nonce + ":cancel\n").encode())
                self.assertEqual(peer.recv(128), b"stopped\n")
            _, error = process.communicate(timeout=5)
            self.assertNotEqual(process.returncode, 0, error)
            with self.assertRaises(ProcessLookupError):
                os.kill(child, 0)
            self.assertFalse(Path(address).parent.exists())

    def test_command_stdin_and_output_survive_the_controller(self):
        process, nonce = self.start_controller("cat", piped=True)
        text, error = process.communicate("data supplied by the user\n", timeout=5)
        self.assertEqual(process.returncode, 0, error)
        self.assertEqual(text, "data supplied by the user\n")
        self.assertFalse(Path("/tmp/cd-ssh-" + nonce).exists())

    def test_wrong_nonce_cannot_cancel_another_commands_process_group(self):
        process, nonce = self.start_controller("sleep 60")
        address = "/tmp/cd-ssh-" + nonce + "/control"
        self.wait_until(lambda: Path(address).exists())
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as peer:
            peer.settimeout(5)
            peer.connect(address)
            peer.sendall((uuid.uuid4().hex + ":cancel\n").encode())
            self.assertEqual(peer.recv(128), b"")
        self.assertIsNone(process.poll())


if __name__ == "__main__":
    unittest.main()
