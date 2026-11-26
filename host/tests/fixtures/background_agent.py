"""ACP agent that starts a background program, as a coding agent does with a dev server. It writes its
own PID to agent.pid and the program's to background.pid in its folder, and answers each prompt "ok"."""

import json
import os
import subprocess
import sys


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


for line in sys.stdin:
    message = json.loads(line)
    method, rid = message.get("method"), message.get("id")
    if method is None or rid is None:
        continue
    if method == "initialize":
        send({"id": rid, "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        background = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(300)"], stdout=subprocess.DEVNULL)
        with open("background.pid", "w", encoding="utf-8") as f:
            f.write(str(background.pid))
        with open("agent.pid", "w", encoding="utf-8") as f:
            f.write(str(os.getpid()))
        send({"id": rid, "result": {"sessionId": "background"}})
    elif method == "session/prompt":
        text = {"type": "text", "text": "ok"}
        update = {"sessionUpdate": "agent_message_chunk", "content": text}
        send({"method": "session/update", "params": {"sessionId": "background", "update": update}})
        send({"id": rid, "result": {"stopReason": "end_turn"}})
    else:
        send({"id": rid, "result": {}})
