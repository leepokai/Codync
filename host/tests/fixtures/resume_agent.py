"""ACP agent that can't load a session but can resume one, like DeepSeek Harness: no `loadSession`, `session/load` is
an unknown method, and resuming a session it's still holding is refused. While the file model.refused is in its folder
it refuses the bot's model. It logs every request to requests.jsonl in its folder."""

import json
import os
import sys
import uuid

MODELS = {"configOptions": [{"id": "model", "category": "model", "type": "select", "currentValue": "fixture",
                             "options": [{"value": "fixture", "name": "Fixture"}]}]}

holding = set()


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


for line in sys.stdin:
    message = json.loads(line)
    method, rid = message.get("method"), message.get("id")
    if method is None or rid is None:
        continue
    params = message.get("params", {})
    with open("requests.jsonl", "a", encoding="utf-8") as log:
        log.write(json.dumps({"method": method, "params": params}) + "\n")
    if method == "initialize":
        capabilities = {"sessionCapabilities": {"resume": {}, "close": {}}}
        send({"id": rid, "result": {"protocolVersion": 1, "agentCapabilities": capabilities}})
    elif method == "session/new":
        sid = str(uuid.uuid4())
        holding.add(sid)
        send({"id": rid, "result": {"sessionId": sid, **MODELS}})
    elif method == "session/resume":
        if params["sessionId"] in holding:
            send({"id": rid, "error": {"code": -32602, "message": "session is already active"}})
        else:
            holding.add(params["sessionId"])
            send({"id": rid, "result": MODELS})
    elif method == "session/set_config_option" and os.path.exists("model.refused"):
        send({"id": rid, "error": {"code": -32602, "message": "unknown model option"}})
    elif method == "session/set_config_option":
        send({"id": rid, "result": MODELS})
    elif method == "session/close":
        holding.discard(params["sessionId"])
        send({"id": rid, "result": {}})
    elif method == "session/prompt":
        update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "ok"}}
        send({"method": "session/update", "params": {"sessionId": params["sessionId"], "update": update}})
        send({"id": rid, "result": {"stopReason": "end_turn"}})
    else:
        send({"id": rid, "error": {"code": -32601, "message": f"Method not found: {method}"}})
