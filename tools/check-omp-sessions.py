#!/usr/bin/env python3
"""Runs the host's memory, network, routines, turns, files and team end-to-end suites, then fails if
any of their hosts left a session in the user's oh-my-pi sessions folder.

Run from anywhere: python3 tools/check-omp-sessions.py. Needs oh-my-pi installed to mean anything.
The suites run with TMPDIR set to a folder of this run, so every test home is in it. A session folder
that is new after the run and holds a session whose working directory is in that folder is theirs:
sessions other oh-my-pi processes open meanwhile never count.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SUITES = ["memory_e2e", "network_e2e", "routines_e2e", "turns_e2e", "files_e2e", "team_e2e"]


def sessions_folder():
    agent_dir = os.environ.get("PI_CODING_AGENT_DIR") or Path.home() / ".omp" / "agent"
    return Path(agent_dir) / "sessions"


def folders(sessions):
    return set(sessions.iterdir()) if sessions.is_dir() else set()


def working_dirs(folder):
    """The working directory each session in `folder` was opened in (its first `session` line)."""
    for log in folder.glob("*.jsonl"):
        with log.open(errors="replace") as lines:
            for line in lines:
                entry = json.loads(line)
                if entry.get("type") == "session":
                    yield entry["cwd"]
                    break


def main():
    sessions = sessions_folder()
    before = folders(sessions)
    # Under /tmp, as the suites expect: macOS's own TMPDIR makes the network tests' socket paths too long.
    with tempfile.TemporaryDirectory(prefix="codync-omp-check-", dir="/tmp") as temp:
        args = ["cargo", "test", *[arg for suite in SUITES for arg in ("--test", suite)]]
        run = subprocess.run(args, cwd=ROOT / "host", env={**os.environ, "TMPDIR": temp})
    left = sorted(
        folder.name
        for folder in folders(sessions) - before
        if any(cwd.startswith(temp) for cwd in working_dirs(folder))
    )
    if left:
        sys.exit(f"{len(left)} session folder(s) of this run in {sessions}:\n" + "\n".join(left))
    if run.returncode != 0:
        sys.exit(f"the suites failed (exit {run.returncode})")
    print(f"no session folder of this run in {sessions}")


if __name__ == "__main__":
    main()
