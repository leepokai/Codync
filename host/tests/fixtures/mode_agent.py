"""ACP agent with session modes like Codex's adapter: Read-only, Auto review (id `agent`) and Full access, as a
`mode` config option that starts on Read-only. It logs session calls to calls.jsonl in its folder and answers each
prompt with its mode and the prompt text. With a `planless` file in its folder, a prompt moves the session to
Full access by itself (the way Claude leaves Plan once the plan is approved); with a `stuck` file it refuses
every mode switch; with a `limit` file every prompt fails the way Claude's adapter reports a usage limit."""

import json
import os
import sys

mode = "read-only"


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def log(value):
    with open("calls.jsonl", "a", encoding="utf-8") as f:
        f.write(json.dumps(value) + "\n")


def options():
    return {"configOptions": [{
        "id": "mode", "name": "Mode", "category": "mode", "type": "select", "currentValue": mode,
        "options": [
            {"value": "read-only", "name": "Read-only"},
            {"value": "agent", "name": "Auto review", "description": "Only ask for actions detected as potentially unsafe"},
            {"value": "agent-full-access", "name": "Full access"},
        ],
    }]}


for line in sys.stdin:
    message = json.loads(line)
    method, rid = message.get("method"), message.get("id")
    if method is None or rid is None:
        continue
    params = message.get("params", {})
    if method == "initialize":
        send({"id": rid, "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": True}}})
    elif method in ("session/new", "session/load"):
        log({"method": method})
        send({"id": rid, "result": {"sessionId": "s1", **options()}})
    elif method == "session/set_config_option" and os.path.exists("stuck"):
        send({"id": rid, "error": {"code": -32603, "message": "mode switch refused"}})
    elif method == "session/set_config_option":
        log({"method": method, "configId": params["configId"], "value": params["value"]})
        mode = params["value"]
        send({"id": rid, "result": options()})
    elif method == "session/prompt" and os.path.exists("limit"):
        limit = {"code": -32603, "message": "Internal error: You've hit your limit", "data": {"errorKind": "rate_limit"}}
        send({"id": rid, "error": limit})
    elif method == "session/prompt":
        text = " ".join(b.get("text", "") for b in params.get("prompt", []))
        update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"mode {mode} · {text}"}}
        send({"method": "session/update", "params": {"sessionId": "s1", "update": update}})
        if os.path.exists("planless"):
            mode = "agent-full-access"
            update = {"sessionUpdate": "config_option_update", **options()}
            send({"method": "session/update", "params": {"sessionId": "s1", "update": update}})
        send({"id": rid, "result": {"stopReason": "end_turn"}})
    else:
        send({"id": rid, "result": {}})
