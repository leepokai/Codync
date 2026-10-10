"""ACP agent stuck mid-turn: it never answers a prompt and ignores session/cancel."""

import json
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
        text = {"type": "text", "text": "Working on it"}
        update = {"sessionUpdate": "agent_message_chunk", "content": text}
        send({"method": "session/update", "params": {"sessionId": "stuck", "update": update}})
