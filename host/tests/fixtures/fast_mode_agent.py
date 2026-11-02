"""ACP agent offering Fast mode like Claude's adapter: a `fast` select that is off in
every new process. It resumes sessions, logs session calls to calls.jsonl in its
folder, and answers each prompt with its speed and the prompt text. With a `refuse`
file in its folder it turns Fast mode back off during a prompt, the way Claude does
for an account without extra usage. With a `plain` file its sessions list no config
options at all; with a `moded` file they also list modes like Codex's, starting on Read-only."""

import json
import os
import sys

fast = "off"
mode = "read-only"


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def log(value):
    with open("calls.jsonl", "a", encoding="utf-8") as f:
        f.write(json.dumps(value) + "\n")


def options(description="Faster responses on supported models"):
    listed = [{
        "id": "fast", "name": "Fast mode", "description": description, "category": "model_config",
        "type": "select", "currentValue": fast,
        "options": [{"value": "on", "name": "On"}, {"value": "off", "name": "Off"}],
    }]
    if os.path.exists("moded"):
        listed.append({"id": "mode", "name": "Mode", "category": "mode", "type": "select", "currentValue": mode,
                       "options": [{"value": "read-only", "name": "Read-only"}, {"value": "agent", "name": "Auto review"}]})
    return {"configOptions": listed}


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
        listed = {} if os.path.exists("plain") else options()
        send({"id": rid, "result": {"sessionId": "s1", **listed}})
    elif method == "session/set_config_option":
        log({"method": method, "configId": params["configId"], "value": params["value"]})
        if params["configId"] == "mode":
            mode = params["value"]
        else:
            fast = params["value"]
        send({"id": rid, "result": options()})
    elif method == "session/prompt":
        if fast == "on" and os.path.exists("refuse"):
            fast = "off"
            update = {"sessionUpdate": "config_option_update",
                      **options("Faster responses on supported models — requires extra usage")}
            send({"method": "session/update", "params": {"sessionId": "s1", "update": update}})
        text = " ".join(b.get("text", "") for b in params.get("prompt", []))
        update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"fast {fast} · {text}"}}
        send({"method": "session/update", "params": {"sessionId": "s1", "update": update}})
        send({"id": rid, "result": {"stopReason": "end_turn"}})
    else:
        send({"id": rid, "result": {}})
