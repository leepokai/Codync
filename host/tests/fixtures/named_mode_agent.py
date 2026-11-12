"""ACP agent `named_mode_agent.py <name> <mode ids...>` for fallback chains: lists those modes as a `mode` config
option (it opens in the first; id `agent` is named "Auto review", Codex's Auto) and logs every request to
<name>.jsonl in its folder. While <name>.limit exists every prompt fails with a usage limit, and while <name>.stuck
exists it refuses every mode switch; otherwise it answers "<name> mode <mode>"."""

import json
import os
import sys

NAME, IDS = sys.argv[1], sys.argv[2:]
mode = IDS[0]


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def log(value):
    with open(f"{NAME}.jsonl", "a", encoding="utf-8") as f:
        f.write(json.dumps(value) + "\n")


def options():
    listed = [{"value": i, "name": "Auto review" if i == "agent" else i} for i in IDS]
    return {"configOptions": [{"id": "mode", "name": "Mode", "category": "mode", "type": "select",
                               "currentValue": mode, "options": listed}]}


for line in sys.stdin:
    message = json.loads(line)
    method, rid = message.get("method"), message.get("id")
    if method is None or rid is None:
        continue
    params = message.get("params", {})
    log({"method": method, "params": params})
    if method == "initialize":
        send({"id": rid, "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": True}}})
    elif method in ("session/new", "session/load"):
        send({"id": rid, "result": {"sessionId": "s1", **options()}})
    elif method == "session/set_config_option" and os.path.exists(f"{NAME}.stuck"):
        send({"id": rid, "error": {"code": -32603, "message": "mode switch refused"}})
    elif method == "session/set_config_option":
        mode = params["value"]
        send({"id": rid, "result": options()})
    elif method == "session/prompt" and os.path.exists(f"{NAME}.limit"):
        limit = {"code": -32603, "message": "Internal error: You've hit your limit", "data": {"errorKind": "rate_limit"}}
        send({"id": rid, "error": limit})
    elif method == "session/prompt":
        update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"{NAME} mode {mode}"}}
        send({"method": "session/update", "params": {"sessionId": "s1", "update": update}})
        send({"id": rid, "result": {"stopReason": "end_turn"}})
    else:
        send({"id": rid, "result": {}})
