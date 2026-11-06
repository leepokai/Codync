"""ACP agent for the fallback chain test, run as `relay_agent.py <name>`. While `<name>.limit` exists in its folder,
every prompt edits a file and then fails the way Claude's adapter reports a usage limit; otherwise it answers
"<name> answered". While `<name>.signedout` exists, every prompt fails asking to sign in (ACP's -32000); while
`<name>.revoked` exists, only a process started before it does (it holds the sign-in that file replaced). While
`<name>.refusal` exists, every prompt answers `refusal` with the file's text as `_meta["codebuddy.ai/errorMessage"]`, the
way CodeBuddy's CLI reports an error (an empty file: a plain refusal). Sessions load (`<name>-s<n>`) unless
`<name>.noload` exists; every request goes to `<name>.jsonl` in its folder."""

import json
import os
import sys
import time

STARTED = time.time()
NAME = sys.argv[1]
LIMIT = {
    "code": -32603,
    "message": "Internal error: You've hit your limit · resets 3pm (Asia/Taipei)",
    "data": {"errorKind": "rate_limit"},
}
sessions = 0


def send(value):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
    sys.stdout.flush()


def update(sid, update):
    send({"method": "session/update", "params": {"sessionId": sid, "update": update}})


for line in sys.stdin:
    message = json.loads(line)
    method, params = message.get("method"), message.get("params") or {}
    if "id" not in message or method is None:
        continue
    with open(f"{NAME}.jsonl", "a", encoding="utf-8") as log:
        log.write(json.dumps({"method": method, "params": params}) + "\n")
    if method == "initialize":
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": True}}})
    elif method == "session/new":
        sessions += 1
        send({"id": message["id"], "result": {"sessionId": f"{NAME}-s{sessions}"}})
    elif method == "session/load" and os.path.exists(f"{NAME}.noload"):
        send({"id": message["id"], "error": {"code": -32603, "message": "Internal error: session not found"}})
    elif method == "session/load":
        send({"id": message["id"], "result": {}})
    elif method == "session/prompt":
        sid = params["sessionId"]
        revoked = os.path.exists(f"{NAME}.revoked") and os.path.getmtime(f"{NAME}.revoked") > STARTED
        if os.path.exists(f"{NAME}.signedout") or revoked:
            send({"id": message["id"], "error": {"code": -32000, "message": "Authentication required"}})
        elif os.path.exists(f"{NAME}.refusal"):
            with open(f"{NAME}.refusal", encoding="utf-8") as f:
                error = f.read()
            meta = {"_meta": {"codebuddy.ai/errorMessage": error}} if error else {}
            send({"id": message["id"], "result": {"stopReason": "refusal", **meta}})
        elif os.path.exists(f"{NAME}.limit"):
            tool = {"toolCallId": "t1", "title": "Edit a.txt", "kind": "edit", "status": "completed"}
            update(sid, {"sessionUpdate": "tool_call", **tool})
            update(sid, {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Edited a.txt"}})
            send({"id": message["id"], "error": LIMIT})
        else:
            update(sid, {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"{NAME} answered"}})
            send({"id": message["id"], "result": {"stopReason": "end_turn"}})
    else:
        send({"id": message["id"], "error": {"code": -32601, "message": "not supported"}})
