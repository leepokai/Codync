"""Small ACP fixture for non-interactive send, permission, and cancellation tests."""

import json
import re
import sys
import threading
import time

sys.stdin.reconfigure(encoding="utf-8")
sys.stdout.reconfigure(encoding="utf-8")
lock = threading.Lock()
permissions = {}
prompts = {}
finishers = set()
sessions = 0


def send(value):
    with lock:
        print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def update(session, kind, text):
    send({"method": "session/update", "params": {
        "sessionId": session,
        "update": {"sessionUpdate": kind, "content": {"type": "text", "text": text}},
    }})


def turn(message):
    rid = message["id"]
    session = message["params"]["sessionId"]
    text = "\n".join(block.get("text", "") for block in message["params"]["prompt"])
    # A quote reply's note repeats the quoted message; only the latest command is current.
    modes = re.findall(r"CLI_MODE=([a-z_]+)", text)
    mode = modes[-1] if modes else "normal"
    event = threading.Event()
    prompts[rid] = (session, event)
    if mode in {"hang", "finish_on_cancel"}:
        if mode == "finish_on_cancel":
            finishers.add(rid)
        update(session, "agent_thought_chunk", "Waiting indefinitely")
        event.wait(120)
        return
    if mode == "error":
        send({"id": rid, "error": {"code": -32000, "message": "fixture failure"}})
        prompts.pop(rid, None)
        return
    if mode == "silent":
        send({"id": rid, "result": {"stopReason": "end_turn"}})
        prompts.pop(rid, None)
        return
    if mode == "hold":
        update(session, "agent_thought_chunk", "OLD is working")
        time.sleep(3)
        reply = "OLD reply"
    elif mode == "permission":
        permission_id = f"permission-{rid}"
        permissions[permission_id] = event
        send({"id": permission_id, "method": "session/request_permission", "params": {
            "sessionId": session,
            "toolCall": {"toolCallId": "permission-tool", "title": "Read fixture files", "kind": "read"},
            "options": [
                {"optionId": "allow", "name": "Allow once", "kind": "allow_once"},
                {"optionId": "reject", "name": "Reject once", "kind": "reject_once"},
            ],
        }})
        if not event.wait(90):
            return
        reply = "Approved reply"
    elif mode == "echo":
        reply = re.findall(r"CLI_TEXT=([^\n]+)", text)[-1]
    else:
        reply = "CLI reply"
    update(session, "agent_message_chunk", reply)
    reason = mode if mode in {"max_tokens", "max_turn_requests", "refusal"} else "end_turn"
    send({"id": rid, "result": {"stopReason": reason}})
    prompts.pop(rid, None)


for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method is None:
        event = permissions.pop(message.get("id"), None)
        if event:
            event.set()
    elif method == "initialize":
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {}}})
    elif method == "session/new":
        sessions += 1
        send({"id": message["id"], "result": {"sessionId": f"cli-session-{sessions}"}})
    elif method == "session/prompt":
        threading.Thread(target=turn, args=(message,), daemon=True).start()
    elif method == "session/cancel":
        for rid, (session, event) in list(prompts.items()):
            if session == message["params"]["sessionId"]:
                prompts.pop(rid, None)
                # Some agents finish the turn normally when asked to cancel.
                if rid in finishers:
                    update(session, "agent_message_chunk", "Finished anyway")
                send({"id": rid, "result": {"stopReason": "end_turn" if rid in finishers else "cancelled"}})
                event.set()
