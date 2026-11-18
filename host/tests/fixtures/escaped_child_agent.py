"""ACP agent stuck mid-turn that ignores session/cancel and, like a shell tool's detached server, leaves a process of
its own session behind that still holds the agent's stdout: stopping the agent's tree never closes the pipe (#466).
Told "leave", it exits right after starting that process instead, mid-turn."""

import json
import os
import subprocess
import sys


def send(value):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method == "initialize":
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        send({"id": message["id"], "result": {"sessionId": "stuck"}})
    elif method == "session/prompt":
        # Inherits stdout; its own session is out of the agent's reach. The test kills it by the pid it leaves.
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(300)"], start_new_session=True)
        with open(os.path.join(os.getcwd(), "escaped.pid"), "w") as f:
            f.write(str(child.pid))
        text = {"type": "text", "text": "Working on it"}
        update = {"sessionUpdate": "agent_message_chunk", "content": text}
        send({"method": "session/update", "params": {"sessionId": "stuck", "update": update}})
        if message["params"]["prompt"][-1]["text"].endswith("\nleave"):
            os._exit(1)
