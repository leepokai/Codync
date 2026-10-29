"""ACP fixture exposing effort with legacy model selection and per-process defaults."""
import json
import sys

effort = "medium"
model = "default"


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def options():
    return {"configOptions": [{"id": "effort", "category": "thought_level", "type": "select",
        "currentValue": effort, "options": [{"value": v, "name": v} for v in ["low", "medium", "high"]]}]}


for line in sys.stdin:
    message = json.loads(line)
    method, rid = message.get("method"), message.get("id")
    if method is None or rid is None:
        continue
    params = message.get("params", {})
    if method == "initialize":
        result = {"protocolVersion": 1, "agentCapabilities": {"loadSession": True}}
    elif method in ("session/new", "session/load"):
        result = {"sessionId": "s1", **options()}
    elif method == "session/set_model":
        model = params["modelId"]
        result = {}
    elif method == "session/set_config_option":
        assert params["configId"] == "effort"
        effort = params["value"]
        result = options()
    elif method == "session/prompt":
        text = " ".join(b.get("text", "") for b in params.get("prompt", []))
        send({"method": "session/update", "params": {"sessionId": "s1", "update": {
            "sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"{model} {effort} · {text}"}}}})
        result = {"stopReason": "end_turn"}
    else:
        result = {}
    send({"id": rid, "result": result})
