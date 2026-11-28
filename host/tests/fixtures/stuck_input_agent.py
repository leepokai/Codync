"""ACP agent that accepts a session, then leaves its input unread until killed."""

import json
import pathlib
import sys
import threading


def send(value):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method == "initialize":
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        send({"id": message["id"], "result": {"sessionId": "stuck-input"}})
        pathlib.Path("session-ready").write_text("", encoding="utf-8")
        # The next prompt remains in the pipe: a large one blocks its writer.
        threading.Event().wait()
