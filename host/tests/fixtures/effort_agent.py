"""ACP agent offering reasoning effort the way Codex's adapter does: a session-level
`reasoning_effort` select (category `thought_level`) beside the model select. Model
`sol` accepts low and high, `mini` only low; a value the model doesn't list is
rejected. Each process starts on sol at low, like a fresh Codex. It resumes sessions,
logs session calls to calls.jsonl in its folder, and answers each prompt with its
model, effort and the message."""

import json
import sys

model, effort = "sol", "low"
EFFORTS = {"sol": ["low", "high"], "mini": ["low"]}


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def log(value):
    with open("calls.jsonl", "a", encoding="utf-8") as f:
        f.write(json.dumps(value) + "\n")


def options():
    return {"configOptions": [
        {"id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": model,
         "options": [{"value": "sol", "name": "Sol"}, {"value": "mini", "name": "Mini"}]},
        {"id": "reasoning_effort", "name": "Reasoning Effort", "category": "thought_level", "type": "select",
         "currentValue": effort, "options": [{"value": v, "name": v.title()} for v in EFFORTS[model]]},
    ]}


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
    elif method == "session/set_config_option":
        config, value = params["configId"], params["value"]
        log({"method": method, "configId": config, "value": value})
        if config == "model" and value in EFFORTS:
            model, effort = value, "low"
        elif config == "reasoning_effort" and value in EFFORTS[model]:
            effort = value
        else:
            send({"id": rid, "error": {"code": -32602, "message": f"{config} {value} is not available"}})
            continue
        send({"id": rid, "result": options()})
    elif method == "session/prompt":
        # The first prompt carries the bot's profile: answer to the message itself, its last line.
        text = " ".join(b.get("text", "") for b in params.get("prompt", [])).strip().splitlines()[-1]
        update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"{model} {effort} · {text}"}}
        send({"method": "session/update", "params": {"sessionId": "s1", "update": update}})
        send({"id": rid, "result": {"stopReason": "end_turn"}})
    else:
        send({"id": rid, "result": {}})
