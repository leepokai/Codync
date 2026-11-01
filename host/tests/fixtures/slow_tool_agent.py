"""ACP agent whose turn runs one long tool call: it reports the call in progress, says nothing while it runs for
`<seconds>` (`slow_tool_agent.py <seconds>`), then completes it and answers "built"."""

import json
import sys
import time

SECONDS = float(sys.argv[1])


def send(value):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
    sys.stdout.flush()


def update(sid, update):
    send({"method": "session/update", "params": {"sessionId": sid, "update": update}})


for line in sys.stdin:
    message = json.loads(line)
    method, params = message.get("method"), message.get("params") or {}
    if "id" not in message or method is None:
        continue
    if method == "initialize":
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        send({"id": message["id"], "result": {"sessionId": "build"}})
    elif method == "session/prompt":
        tool = {"toolCallId": "t1", "title": "cargo build", "kind": "execute"}
        update("build", {"sessionUpdate": "tool_call", "status": "in_progress", **tool})
        time.sleep(SECONDS)
        update("build", {"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed"})
        update("build", {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "built"}})
        send({"id": message["id"], "result": {"stopReason": "end_turn"}})
    else:
        send({"id": message["id"], "error": {"code": -32601, "message": "not supported"}})
