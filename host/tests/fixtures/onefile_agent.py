"""A PyInstaller onefile agent: a launcher unpacks into a temp folder, runs the agent as its child and, once that child
has ended, takes a while to remove the folder. The agent ignores its closed stdin, like Antigravity's Windows server,
and runs a tool that runs a program of its own; neither the agent nor the tool ends when what it started does."""

import json
import os
import shutil
import subprocess
import sys
import time


def start(role):
    return subprocess.Popen([sys.executable, "-u", __file__, role], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL)


role = sys.argv[1]
if role == "agent":
    tool = start("tool")
    print(json.dumps({"jsonrpc": "2.0", "method": "ready"}), flush=True)
    tool.wait()
    time.sleep(60)
elif role == "tool":
    start("program").wait()
    time.sleep(60)
elif role == "program":
    time.sleep(60)
else:
    unpacked = role
    os.makedirs(unpacked)
    subprocess.run([sys.executable, "-u", __file__, "agent"], stdin=sys.stdin, stdout=sys.stdout)
    # Removing its ~6,200 files takes the real launcher a moment.
    time.sleep(2)
    shutil.rmtree(unpacked)
