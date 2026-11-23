"""ACP agent that asks the user a question mid-turn with `elicitation/create`, the way Hermes sends its clarify
prompts: one `answer` field, free text (ASK_TEXT), one of a few options (ASK_CHOICE), yes or no (ASK_YESNO) or several
options (ASK_MULTI). A prompt saying ELSEWHERE asks for a session the host never opened. The turn waits for the reply,
logs it to responses.jsonl in its folder and ends saying what came back. Each request it gets goes to requests.jsonl."""

import json
import sys

FIELDS = {
    "ASK_TEXT": {"type": "string"},
    "ASK_CHOICE": {"type": "string", "enum": ["sqlite", "postgres"], "default": "postgres"},
    "ASK_YESNO": {"type": "boolean"},
    "ASK_MULTI": {"type": "array", "items": {"type": "string", "enum": ["lint", "test"]}},
}

waiting = None  # (prompt id, question id) while the turn waits for the reply
cancelled = False


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def log(name, value):
    with open(name, "a", encoding="utf-8") as f:
        f.write(json.dumps(value) + "\n")


def say(text):
    update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}
    send({"method": "session/update", "params": {"sessionId": "s1", "update": update}})


for line in sys.stdin:
    message = json.loads(line)
    method, rid = message.get("method"), message.get("id")
    if method is None and waiting is not None and rid == waiting[1]:
        log("responses.jsonl", message)
        say("Reply: " + json.dumps(message.get("result")))
        send({"id": waiting[0], "result": {"stopReason": "cancelled" if cancelled else "end_turn"}})
        waiting, cancelled = None, False
        continue
    if method == "session/cancel":
        cancelled = waiting is not None
        continue
    if method is None or rid is None:
        continue
    log("requests.jsonl", message)
    params = message.get("params", {})
    if method == "initialize":
        send({"id": rid, "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": True}}})
    elif method in ("session/new", "session/load"):
        send({"id": rid, "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        text = " ".join(b.get("text", "") for b in params.get("prompt", []))
        field = next((f for word, f in FIELDS.items() if word in text), None)
        if field is None:
            say("Nothing to ask.")
            send({"id": rid, "result": {"stopReason": "end_turn"}})
            continue
        waiting = (rid, 700 + rid if isinstance(rid, int) else 700)
        session = "s9" if "ELSEWHERE" in text else "s1"
        schema = {"type": "object", "properties": {"answer": field}, "required": ["answer"]}
        send({"id": waiting[1], "method": "elicitation/create",
              "params": {"sessionId": session, "mode": "form", "message": "Which database?", "requestedSchema": schema}})
    else:
        send({"id": rid, "result": {}})
