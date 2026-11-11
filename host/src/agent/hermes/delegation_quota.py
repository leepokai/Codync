"""Source: Hermes tools/delegate_tool.py child construction contract (MIT, 10d25f8a).
Adaptation: OS-owned per-bot child slots shared by chat and routine execution owners.
"""
from __future__ import annotations

import errno
import functools
import os
from pathlib import Path
import time


def acquire(directory: Path, maximum: int):
    """Return a locked file handle, or None at capacity. Process exit releases it."""
    for index in range(maximum):
        handle = (directory / f"{index}.slot").open("a+b")
        try:
            if os.name == "nt":
                import msvcrt
                if os.fstat(handle.fileno()).st_size == 0:
                    handle.write(b"\0")
                    handle.flush()
                handle.seek(0)
                msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            return handle
        except OSError as error:
            handle.close()
            if error.errno not in {errno.EACCES, errno.EAGAIN, errno.EDEADLK}:
                raise
        except BaseException:
            handle.close()
            raise
    return None


def install() -> None:
    from tools import delegate_tool
    from tools.delegate_tool_config import _get_max_concurrent_children, _get_child_timeout

    raw = os.environ.get("CODYNC_HERMES_CHILD_SLOTS", "")
    directory = Path(raw)
    if not raw or not directory.is_absolute():
        raise ValueError("The host must supply a private per-bot child quota directory")
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    original = delegate_tool._build_child_agent

    @functools.wraps(original)
    def build(*args, **kwargs):
        child = original(*args, **kwargs)
        run = child.run_conversation
        maximum = min(10, max(1, _get_max_concurrent_children()))
        timeout = _get_child_timeout()
        wait = min(3600.0, max(30.0, float(timeout or 300)))

        @functools.wraps(run)
        def bounded(*call_args, **call_kwargs):
            deadline = time.monotonic() + wait
            lease = None
            while lease is None:
                parent_ref = getattr(child, "_delegate_parent_ref", None)
                parent = parent_ref() if parent_ref else None
                if getattr(child, "_interrupt_requested", False) or getattr(parent, "_interrupt_requested", False):
                    raise RuntimeError("Subagent was cancelled while waiting for this bot's capacity")
                lease = acquire(directory, maximum)
                if lease is not None:
                    break
                # A parent already occupies a slot. Never deadlock a full delegation
                # tree waiting for nested children: refuse that child explicitly.
                if getattr(child, "_delegate_depth", 1) > 1 or time.monotonic() >= deadline:
                    raise RuntimeError("This bot's subagent capacity is full; wait for existing children")
                time.sleep(0.1)
            try:
                return run(*call_args, **call_kwargs)
            finally:
                lease.close()

        child.run_conversation = bounded
        return child

    delegate_tool._build_child_agent = build
