"""ACP agent offering reasoning effort the way Codex's adapter does: a session-level
`reasoning_effort` select (category `thought_level`) beside the model select. Model
`sol` lists low, high and max, `mini` only low; a value the model doesn't list is
rejected, and so is max, the way an account can refuse a listed value. Each process
starts on sol at low, like a fresh Codex. It resumes sessions,
logs session calls to calls.jsonl in its folder, and answers each prompt with its
model, effort and the message. Each new session gets its own id (s1, s2…).

Files in its folder change how it behaves, for lifecycle tests:
- `silent`: a setter succeeds but answers without its config options.
- `foreign`: after each prompt, a config_option_update arrives for another session.
- `exit`: the process exits right after answering a prompt.
- `unlisted`: sessions list the model select only, no settings.
- `hold`: a prompt writes `prompted`, then waits for a `release` file before answering.
- `bare`: new and loaded sessions list no config options at all.

Run as `effort_agent.py <name>` for a bot's fallback chain: it logs to `<name>.calls.jsonl`, and while
`<name>.limit` exists every prompt fails the way Claude's adapter reports a usage limit."""

import json
import os
import sys
import time

NAME = sys.argv[1] if len(sys.argv) > 1 else None
CALLS = f"{NAME}.calls.jsonl" if NAME else "calls.jsonl"
LIMIT = {
    "code": -32603,
    "message": "Internal error: You've hit your limit · resets 3pm (Asia/Taipei)",
    "data": {"errorKind": "rate_limit"},
}
model, effort = "sol", "low"
sessions = 0
EFFORTS = {"sol": ["low", "high", "max"], "mini": ["low"]}


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def log(value):
    with open(CALLS, "a", encoding="utf-8") as f:
        f.write(json.dumps(value) + "\n")


def options():
    if os.path.exists("bare"):
        return {}
    if os.path.exists("unlisted"):
        return {"configOptions": [
            {"id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": model,
             "options": [{"value": "sol", "name": "Sol"}, {"value": "mini", "name": "Mini"}]}]}
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
    elif method == "session/new":
        sessions += 1
        log({"method": method})
        send({"id": rid, "result": {"sessionId": f"s{sessions}", **options()}})
    elif method == "session/load":
        log({"method": method})
        send({"id": rid, "result": {"sessionId": params.get("sessionId", "s1"), **options()}})
    elif method == "session/set_config_option":
        config, value = params["configId"], params["value"]
        log({"method": method, "configId": config, "value": value})
        if config == "model" and value in EFFORTS:
            model, effort = value, "low"
        elif config == "reasoning_effort" and value in EFFORTS[model] and value != "max":
            effort = value
        else:
            send({"id": rid, "error": {"code": -32602, "message": f"{config} {value} is not available"}})
            continue
        send({"id": rid, "result": {} if os.path.exists("silent") else options()})
    elif method == "session/prompt" and NAME and os.path.exists(f"{NAME}.limit"):
        send({"id": rid, "error": LIMIT})
    elif method == "session/prompt":
        if os.path.exists("hold"):
            open("prompted", "w").close()
            while not os.path.exists("release"):
                time.sleep(0.05)
            os.remove("release")
        # The first prompt carries the bot's profile: answer to the message itself, its last line.
        text = " ".join(b.get("text", "") for b in params.get("prompt", [])).strip().splitlines()[-1]
        update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"{model} {effort} · {text}"}}
        sid = params.get("sessionId", "s1")
        send({"method": "session/update", "params": {"sessionId": sid, "update": update}})
        send({"id": rid, "result": {"stopReason": "end_turn"}})
        if os.path.exists("foreign"):
            other = {"sessionUpdate": "config_option_update", "configOptions": [
                {"id": "reasoning_effort", "type": "select", "currentValue": "max", "options": [{"value": "max"}]}]}
            send({"method": "session/update", "params": {"sessionId": "elsewhere", "update": other}})
        if os.path.exists("exit"):
            sys.exit(0)
    else:
        send({"id": rid, "result": {}})
