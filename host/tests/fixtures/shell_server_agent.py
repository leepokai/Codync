"""ACP agent that, like a coding agent's shell tool, leaves a web server running as its own child. Run as
`shell_server_agent.py <port>`; port 0 starts none. It writes its PID to agent.pid in its folder and answers each
prompt "ok"."""

import json
import os
import subprocess
import sys

PORT = int(sys.argv[1])
server = None


def send(value):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
    sys.stdout.flush()


with open("agent.pid", "w", encoding="utf-8") as pid:
    pid.write(str(os.getpid()))

for line in sys.stdin:
    message = json.loads(line)
    method, params = message.get("method"), message.get("params") or {}
    if "id" not in message or method is None:
        continue
    if method == "initialize":
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": True}}})
    elif method == "session/new":
        send({"id": message["id"], "result": {"sessionId": "s1"}})
    elif method == "session/load":
        send({"id": message["id"], "result": {}})
    elif method == "session/prompt":
        if PORT and server is None:
            server = subprocess.Popen(
                [sys.executable, os.path.join(os.path.dirname(os.path.abspath(__file__)), "port_server.py"), str(PORT)],
                # On Windows a child starting on this process's stdin hangs while the loop below is reading it.
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "ok"}}
        send({"method": "session/update", "params": {"sessionId": params["sessionId"], "update": update}})
        send({"id": message["id"], "result": {"stopReason": "end_turn"}})
    else:
        send({"id": message["id"], "result": {}})
