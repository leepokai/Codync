"""Source: Hermes tools/environments/file_sync.py (MIT, 10d25f8a).
Adaptation: strict project-only SSH sync with conflict and archive/path fences.
"""
from __future__ import annotations

import hashlib
import json
import os
import posixpath
from pathlib import Path, PurePosixPath
import shlex
import stat
import subprocess
import tarfile
import tempfile
import threading
import uuid

MAX_FILE = 64 * 1024 * 1024
MAX_TOTAL = 128 * 1024 * 1024
MAX_FILES = 10_000
EXCLUDED_DIRS = frozenset({".git", ".hg", ".svn", ".ssh", ".hermes", ".codync", ".codync-dev",
                          ".aws", ".azure", ".gcloud", "credentials", "vault", "node_modules",
                          "venv", ".venv", "__pycache__", ".gradle", ".idea", ".config",
                          ".npmrc", ".netrc", ".pypirc", "auth.json", "credentials.json",
                          "secrets.json", "secrets.toml", ".git-credentials", ".pgpass",
                          ".docker", ".kube", ".gnupg", ".pki", ".deska", ".npm",
                          "target", "dist", "build", "out", ".next", ".cache", ".pytest_cache"})


def allowed_relative(value: str) -> bool:
    path = PurePosixPath(value)
    if not value or path.is_absolute() or "\\" in value or any(ord(c) < 32 for c in value):
        return False
    for part in path.parts:
        lower = part.lower()
        if part in {"", ".", ".."} or ":" in part or lower in EXCLUDED_DIRS:
            return False
        if lower.startswith((".env", ".codync-", "id_rsa", "id_ed25519", "id_ecdsa")):
            return False
        if lower.endswith((".pem", ".key", ".p12", ".pfx", ".keychain")):
            return False
        if part.endswith((".", " ")) or lower.split(".", 1)[0] in {
            "con", "prn", "aux", "nul", *[f"com{i}" for i in range(1, 10)], *[f"lpt{i}" for i in range(1, 10)]
        }:
            return False
    return str(path) not in {".", ""}


def digest(path: Path) -> str:
    sha = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(65536), b""):
            sha.update(chunk)
    return sha.hexdigest()


# The remote scripts use only Python's standard library; credentials never ride their argv.
REMOTE_INIT = r'''
import json, os, pathlib, stat, sys
owner, nonce = sys.argv[1:]
home = pathlib.Path.home().resolve(strict=True)
base = home / ".codync-workspaces"
for directory in (base, base / owner):
    try:
        directory.mkdir(mode=0o700)
    except FileExistsError:
        pass
    info = directory.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
        raise RuntimeError("SSH project directory must be a private owned directory")
root = base / owner / nonce
root.mkdir(mode=0o700)
with (root / ".codync-owner").open("x") as marker:
    marker.write(nonce)
print(json.dumps({"root": str(root)}))
'''

REMOTE_PATH = r'''
import os, pathlib, stat
root = pathlib.Path(sys.argv[1])
nonce = sys.argv[2]
if root.is_symlink() or (root / ".codync-owner").is_symlink() or (root / ".codync-owner").read_text() != nonce:
    raise RuntimeError("SSH project owner changed")
root = root.resolve(strict=True)
def target(relative):
    path = pathlib.PurePosixPath(relative)
    if path.is_absolute() or not path.parts or any(p in (".", "..") for p in path.parts):
        raise RuntimeError("Invalid SSH project path")
    result = root.joinpath(*path.parts)
    if not result.parent.resolve().is_relative_to(root):
        raise RuntimeError("SSH project parent escaped its workspace")
    current = root
    for part in path.parts:
        current = current / part
        if current.is_symlink():
            raise RuntimeError("SSH project symlinks are not synchronized")
    return result
'''

REMOTE_UPLOAD = "import sys\n" + REMOTE_PATH + r'''
import hashlib, tempfile
destination = target(sys.argv[3])
destination.parent.mkdir(parents=True, exist_ok=True)
destination = target(sys.argv[3])
fd, temporary = tempfile.mkstemp(prefix=".codync-upload-", dir=destination.parent)
try:
    size = 0
    sha = hashlib.sha256()
    with os.fdopen(fd, "wb") as stream:
        while True:
            chunk = sys.stdin.buffer.read(65536)
            if not chunk:
                break
            size += len(chunk)
            if size > 64 * 1024 * 1024:
                raise RuntimeError("SSH project file exceeds 64 MiB")
            stream.write(chunk)
            sha.update(chunk)
    if sha.hexdigest() != sys.argv[4]:
        raise RuntimeError("SSH project upload checksum mismatch")
    if sys.argv[5] not in ("600", "700"):
        raise RuntimeError("Invalid SSH project file mode")
    os.chmod(temporary, int(sys.argv[5], 8))
    os.replace(temporary, target(sys.argv[3]))
    print("uploaded")
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
'''

REMOTE_DELETE = "import sys\n" + REMOTE_PATH + r'''
import json
for relative in json.loads(sys.argv[3]):
    destination = target(relative)
    if destination.exists():
        if not destination.is_file():
            raise RuntimeError("Refusing to remove a non-file project entry")
        destination.unlink()
print("deleted")
'''

REMOTE_ARCHIVE = "import sys\n" + REMOTE_PATH + r'''
import json, tarfile
denied = set(json.loads(sys.argv[3]))
paths = []
total = 0
for directory, directories, files in os.walk(root, followlinks=False):
    directories[:] = [name for name in directories if name.lower() not in denied and not name.lower().startswith((".env", ".codync-")) and not (pathlib.Path(directory) / name).is_symlink()]
    for name in files:
        lower = name.lower()
        if lower in denied or lower.startswith((".env", ".codync-", "id_rsa", "id_ed25519", "id_ecdsa")) or lower.endswith((".pem", ".key", ".p12", ".pfx", ".keychain")):
            continue
        candidate = pathlib.Path(directory) / name
        if candidate.is_symlink() or not candidate.is_file():
            continue
        relative = candidate.relative_to(root).as_posix()
        candidate = target(relative)
        info = candidate.stat()
        if info.st_nlink != 1:
            continue
        size = info.st_size
        total += size
        if size > 64 * 1024 * 1024 or total > 128 * 1024 * 1024 or len(paths) >= 10000:
            raise RuntimeError("SSH project synchronization size limit exceeded")
        paths.append((candidate, relative))
with tarfile.open(fileobj=sys.stdout.buffer, mode="w|") as archive:
    for candidate, relative in paths:
        archive.add(candidate, arcname=relative, recursive=False)
'''


def validate_project_root(project_root):
    local = Path(project_root).resolve(strict=True)
    sensitive = {".git", ".ssh", ".hermes", ".codync", ".codync-dev", ".aws", ".azure", ".gcloud", "credentials", "vault", ".config", ".docker", ".kube", ".gnupg", ".deska"}
    if (not local.is_dir() or local in {Path(local.anchor), Path.home().resolve()}
            or any(part.lower() in sensitive for part in local.parts)):
        raise ValueError("Choose a project directory outside credential storage, not a filesystem or home root")
    return local


class ProjectWorkspace:
    def __init__(self, environment, project_root: Path, owner: str):
        from tools.environments.file_sync import FileSyncManager

        self.environment = environment
        self.local = validate_project_root(project_root)
        self.nonce = uuid.uuid4().hex
        self.lock = threading.RLock()
        self.local_identity = (self.local.stat().st_dev, self.local.stat().st_ino)
        self.snapshot_files = None
        self.remote_dirty = False
        result = environment._run_ssh(shlex.join(["python3", "-c", REMOTE_INIT, hashlib.sha256(owner.encode()).hexdigest()[:24], self.nonce]), timeout=15)
        if result.returncode != 0:
            raise RuntimeError("Could not create the private SSH project workspace")
        self.remote = PurePosixPath(json.loads(result.stdout)["root"])
        if not self.remote.is_absolute() or self.remote.name != self.nonce or ".codync-workspaces" not in self.remote.parts:
            raise RuntimeError("SSH returned an invalid project workspace")
        self.manager = FileSyncManager(get_files_fn=self._snapshot_sources, upload_fn=self.upload, delete_fn=self.delete,
                                       sync_interval=0)

    def _snapshot_sources(self):
        if self.snapshot_files is None:
            raise RuntimeError("SSH project uploads require a captured host snapshot")
        return self.snapshot_files

    def files(self):
        entries = []
        total = 0
        for directory, directories, files in os.walk(self.local, followlinks=False):
            parent = Path(directory)
            directories[:] = [name for name in directories if allowed_relative((parent / name).relative_to(self.local).as_posix()) and not (parent / name).is_symlink()]
            for name in sorted(files):
                path = parent / name
                relative = path.relative_to(self.local).as_posix()
                if not allowed_relative(relative) or path.is_symlink():
                    continue
                info = path.stat()
                if not stat.S_ISREG(info.st_mode) or not path.resolve().is_relative_to(self.local):
                    continue
                total += info.st_size
                if info.st_size > MAX_FILE or total > MAX_TOTAL or len(entries) >= MAX_FILES:
                    raise RuntimeError("SSH project sync supports at most 128 MiB and 10000 regular files (64 MiB per file)")
                entries.append((str(path), str(self.remote / relative)))
        return entries

    def relative(self, path):
        relative = PurePosixPath(path).relative_to(self.remote).as_posix()
        if not allowed_relative(relative):
            raise ValueError("SSH synchronization path is outside the allowed project")
        return relative

    def command(self, script, *args):
        return self.environment._build_ssh_command() + [shlex.join(["python3", "-c", script, str(self.remote), self.nonce, *args])]

    def upload(self, host_path, remote_path):
        relative = self.relative(remote_path)
        with open(host_path, "rb") as stream:
            mode = "700" if os.fstat(stream.fileno()).st_mode & stat.S_IXUSR else "600"
            result = subprocess.run(self.command(REMOTE_UPLOAD, relative, digest(Path(host_path)), mode), stdin=stream,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=45, check=False)
        if result.returncode != 0 or result.stdout.strip() != b"uploaded":
            raise RuntimeError("SSH project upload failed; the command was not started")

    def delete(self, paths):
        relative = [self.relative(path) for path in paths]
        result = subprocess.run(self.command(REMOTE_DELETE, json.dumps(relative)), stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15, check=False)
        if result.returncode != 0 or result.stdout.strip() != b"deleted":
            raise RuntimeError("SSH project deletion sync failed; the command was not started")

    def push(self):
        with self.lock, tempfile.TemporaryDirectory(prefix="codync-ssh-upload-") as temporary:
            self.pull()  # background jobs may have changed files even after an earlier final poll
            staged = []
            expected = {}
            total = 0
            for local, remote in self.files():
                destination = Path(temporary) / str(len(staged))
                size = 0
                sha = hashlib.sha256()
                with self._open_source(Path(local)) as source, destination.open("xb") as output:
                    executable = bool(os.fstat(source.fileno()).st_mode & stat.S_IXUSR)
                    while chunk := source.read(65536):
                        size += len(chunk)
                        total += len(chunk)
                        if size > MAX_FILE or total > MAX_TOTAL:
                            raise RuntimeError("SSH project changed beyond the synchronization size limit")
                        sha.update(chunk)
                        output.write(chunk)
                destination.chmod(0o700 if executable else 0o600)
                staged.append((str(destination), remote))
                expected[remote] = sha.hexdigest()
            self.snapshot_files = staged
            try:
                self.manager.sync(force=True)
                # Native sync logs/rolls back errors; owner execution must fail closed instead.
                if expected != self.manager._pushed_hashes:
                    raise RuntimeError("SSH project synchronization failed; retry after resolving the transfer or local edit")
            finally:
                self.snapshot_files = None

    def _open_source(self, path):
        """Capture a regular project file without following a swapped symlink/credential link."""
        relative = path.relative_to(self.local)
        self._safe_target(path)
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            import msvcrt
            stream = path.open("rb")
            try:
                final_name = ctypes.WinDLL("kernel32", use_last_error=True).GetFinalPathNameByHandleW
                final_name.argtypes = (wintypes.HANDLE, wintypes.LPWSTR, wintypes.DWORD, wintypes.DWORD)
                final_name.restype = wintypes.DWORD
                buffer = ctypes.create_unicode_buffer(32768)
                count = final_name(msvcrt.get_osfhandle(stream.fileno()), buffer, len(buffer), 0)
                if not count or count >= len(buffer):
                    raise RuntimeError("Could not verify the opened SSH project source")
                actual = buffer.value
                actual = "\\\\" + actual[8:] if actual.startswith("\\\\?\\UNC\\") else actual.removeprefix("\\\\?\\")
                if not Path(actual).resolve().is_relative_to(self.local):
                    raise RuntimeError("SSH project source escaped its directory")
                info = os.fstat(stream.fileno())
                if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                    raise RuntimeError("Linked/non-regular project sources are not synchronized")
                return stream
            except BaseException:
                stream.close()
                raise
        root = os.open(self.local, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        directory = root
        try:
            info = os.fstat(root)
            expected = getattr(self, "local_identity", (info.st_dev, info.st_ino))
            if (info.st_dev, info.st_ino) != expected:
                raise RuntimeError("The host project directory was replaced")
            for component in relative.parts[:-1]:
                child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
                if directory != root:
                    os.close(directory)
                directory = child
            fd = os.open(relative.name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=directory)
            try:
                info = os.fstat(fd)
                if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                    raise RuntimeError("Linked/non-regular project sources are not synchronized")
                return os.fdopen(fd, "rb")
            except BaseException:
                os.close(fd)
                raise
        finally:
            if directory != root:
                os.close(directory)
            os.close(root)

    def cwd(self, value):
        text = str(value or self.remote)
        if text in {"~", "~/", "$HOME"}:
            return str(self.remote)
        remote = PurePosixPath(posixpath.normpath(text))
        if remote == self.remote or self.remote in remote.parents:
            return str(remote)
        local = Path(text)
        if not local.is_absolute():
            local = self.local / local
        local = local.resolve()
        if not local.is_relative_to(self.local):
            raise ValueError("SSH cwd must stay inside this bot's project")
        return str(self.remote / local.relative_to(self.local).as_posix())

    def _download(self, destination, timeout=45):
        process = subprocess.Popen(self.command(REMOTE_ARCHIVE, json.dumps(sorted(EXCLUDED_DIRS))), stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        expired = threading.Event()
        def stop():
            expired.set()
            try:
                process.kill()
            except ProcessLookupError:
                pass
        timer = threading.Timer(timeout, stop)
        timer.start()
        try:
            size = 0
            with open(destination, "wb") as output:
                while chunk := process.stdout.read(65536):
                    size += len(chunk)
                    if size > MAX_TOTAL + MAX_FILES * 2048:
                        raise RuntimeError("SSH project archive exceeds its size limit")
                    output.write(chunk)
            if process.wait(timeout=5) != 0 or expired.is_set():
                raise RuntimeError("SSH project download failed or timed out")
        finally:
            timer.cancel()
            if process.poll() is None:
                process.kill()
            process.wait(timeout=5)
            process.stdout.close()

    def pull(self, timeout=None):
        with self.lock, tempfile.TemporaryDirectory(prefix="codync-ssh-project-") as temporary:
            archive_path = Path(temporary) / "project.tar"
            if timeout is None:
                self._download(archive_path)
            else:
                self._download(archive_path, timeout=timeout)
            staged = {}
            total = 0
            with tarfile.open(archive_path, "r:") as archive:
                for entry in archive:
                    relative = PurePosixPath(entry.name).as_posix()
                    if entry.isdir():
                        continue
                    key = relative.casefold()
                    if (not entry.isfile() or not allowed_relative(relative) or key in staged
                            or any(parent.as_posix().casefold() in staged for parent in PurePosixPath(relative).parents)
                            or any(name.startswith(key + "/") for name in staged)):
                        raise RuntimeError("SSH project archive contains an unsafe or duplicate entry")
                    total += entry.size
                    if entry.size < 0 or entry.size > MAX_FILE or total > MAX_TOTAL or len(staged) >= MAX_FILES:
                        raise RuntimeError("SSH project archive exceeds its size limit")
                    stream = archive.extractfile(entry)
                    path = Path(temporary) / str(len(staged))
                    with path.open("xb") as output:
                        while chunk := stream.read(65536):
                            output.write(chunk)
                    stream.close()
                    path.chmod(0o700 if entry.mode & stat.S_IXUSR else 0o600)
                    staged[relative.casefold()] = (relative, path, digest(path))
            self._apply(staged)
            self.remote_dirty = False

    def before_spawn(self):
        with self.lock:
            if getattr(self, "remote_dirty", False):
                self.pull()
            self.remote_dirty = True

    def _apply(self, staged):
        baseline = dict(self.manager._pushed_hashes)
        changes = []
        for relative, source, remote_hash in staged.values():
            remote = str(self.remote / relative)
            original = baseline.get(remote)
            if original == remote_hash:
                continue
            target = self.local / relative
            self._safe_target(target)
            if target.exists() and not target.is_file():
                raise RuntimeError(f"SSH sync conflict: host path is not a regular file: {relative}")
            local_hash = digest(target) if target.is_file() else None
            if local_hash not in (original, remote_hash):
                raise RuntimeError(f"SSH sync conflict: local and remote both changed {relative}")
            changes.append((target, source, local_hash))
        remote_files = {str(self.remote / item[0]) for item in staged.values()}
        for remote, original in baseline.items():
            if remote not in remote_files:
                target = self.local / self.relative(remote)
                self._safe_target(target)
                local_hash = digest(target) if target.is_file() else None
                if target.exists() and (not target.is_file() or local_hash != original):
                    raise RuntimeError(f"SSH sync conflict: local edit versus remote deletion of {self.relative(remote)}")
                changes.append((target, None, local_hash))
        for target, source, expected in changes:
            self._safe_target(target)
            if source is None:
                self._unchanged(target, expected)
                target.unlink(missing_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                self._safe_target(target)
                temporary = target.with_name(".codync-sync-" + uuid.uuid4().hex)
                try:
                    with temporary.open("xb") as output, source.open("rb") as stream:
                        while chunk := stream.read(65536):
                            output.write(chunk)
                    temporary.chmod(0o700 if source.stat().st_mode & stat.S_IXUSR else 0o600)
                    self._unchanged(target, expected)
                    os.replace(temporary, target)
                finally:
                    temporary.unlink(missing_ok=True)
        # Preserve the previous baseline for concurrent host-only edits/deletions: they
        # must still upload/delete on the next push, never become falsely acknowledged.
        for remote in baseline.keys() - remote_files:
            self.manager._synced_files.pop(remote, None)
            self.manager._pushed_hashes.pop(remote, None)
        for relative, _, remote_hash in staged.values():
            target = self.local / relative
            if target.is_file() and digest(target) == remote_hash:
                remote = str(self.remote / relative)
                # Force a fresh native content check; a host edit may race this pull.
                self.manager._synced_files[remote] = (-1, -1, -1, -1)
                self.manager._pushed_hashes[remote] = remote_hash

    def _unchanged(self, target, expected):
        self._safe_target(target)
        if target.exists() and not target.is_file():
            raise RuntimeError(f"SSH sync conflict: host destination changed during download: {target.name}")
        actual = digest(target) if target.is_file() else None
        if actual != expected:
            raise RuntimeError(f"SSH sync conflict: host file changed during download: {target.name}")

    def _safe_target(self, path):
        if not path.resolve().is_relative_to(self.local):
            raise RuntimeError("SSH project destination escaped the host project")
        current = self.local
        for part in path.relative_to(self.local).parts:
            current = current / part
            if current.is_symlink():
                raise RuntimeError("SSH project symlink destinations are not synchronized")
            if current != path and current.exists() and not current.is_dir():
                raise RuntimeError("SSH project parent is not a directory")
