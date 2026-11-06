"""ACP agent whose work goes on after its turn (like Hermes' background subagents): it answers each prompt, ends the
turn, then asks permission to edit a file, for its own session `s1`, or for a session the host never opened when the
prompt says "elsewhere". A prompt saying EARLY asks before its turn ends instead; one saying SLOW ends only when
the host cancels it, and asks after that. It logs each answer the host gives to responses.jsonl in its folder."""

import json
import sys
import time

asked = 900
slow = None  # the id of the prompt that waits for session/cancel


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def ask(session):
    global asked
    asked += 1
    options = [{"optionId": "yes", "name": "Allow", "kind": "allow_once"},
               {"optionId": "no", "name": "Reject", "kind": "reject_once"}]
    tool = {"toolCallId": f"t{asked}", "title": "Write notes.md", "kind": "edit"}
    send({"id": asked, "method": "session/request_permission",
          "params": {"sessionId": session, "toolCall": tool, "options": options}})


for line in sys.stdin:
    message = json.loads(line)
    method, rid = message.get("method"), message.get("id")
    if method is None and rid is not None:
        with open("responses.jsonl", "a", encoding="utf-8") as f:
            f.write(json.dumps(message) + "\n")
        continue
    if method == "session/cancel" and slow is not None:
        send({"id": slow, "result": {"stopReason": "cancelled"}})
        slow = None
        time.sleep(0.5)
        ask("s1")
        continue
    if method is None or rid is None:
        continue
    params = message.get("params", {})
    if method == "initialize":
        send({"id": rid, "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": True}}})
    elif method in ("session/new", "session/load"):
        send({"id": rid, "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        text = " ".join(b.get("text", "") for b in params.get("prompt", []))
        update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Started it in the background."}}
        send({"method": "session/update", "params": {"sessionId": "s1", "update": update}})
        if "SLOW" in text:
            slow = rid
            continue
        if "EARLY" in text:
            ask("s1")
        send({"id": rid, "result": {"stopReason": "end_turn"}})
        if "EARLY" not in text:
            time.sleep(0.5)  # the background work takes a while
            ask("s9" if "elsewhere" in text else "s1")
    else:
        send({"id": rid, "result": {}})
