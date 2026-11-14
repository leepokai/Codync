"""ACP agent whose session can end under it, like Claude's adapter when its query stream stops:
"end-this-session" fails the turn, and every later prompt on that session fails with the same uncategorized
internal error until the session is closed. Loading a session that's still open keeps it as it is."""

import json
import sys

ENDED = {"code": -32603, "message": "The Claude Agent session has ended. Please start a new session."}
open_sessions = {}  # id -> ended


def send(value):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
    sys.stdout.flush()


def say(sid, text):
    update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}
    send({"method": "session/update", "params": {"sessionId": sid, "update": update}})


for line in sys.stdin:
    message = json.loads(line)
    method, params = message.get("method"), message.get("params") or {}
    if "id" not in message or method is None:
        continue
    if method == "initialize":
        capabilities = {"loadSession": True, "sessionCapabilities": {"close": {}}}
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": capabilities}})
    elif method == "session/new":
        open_sessions["s1"] = False
        send({"id": message["id"], "result": {"sessionId": "s1"}})
    elif method == "session/load":
        open_sessions.setdefault(params["sessionId"], False)
        send({"id": message["id"], "result": {}})
    elif method == "session/close":
        open_sessions.pop(params["sessionId"], None)
        send({"id": message["id"], "result": {}})
    elif method == "session/prompt":
        sid = params["sessionId"]
        text = " ".join(b.get("text", "") for b in params["prompt"])
        if open_sessions.get(sid, True):
            send({"id": message["id"], "error": ENDED})
        elif "end-this-session" in text:
            open_sessions[sid] = True
            send({"id": message["id"], "error": ENDED})
        else:
            say(sid, "still here" if "still there?" in text else "pong")
            send({"id": message["id"], "result": {"stopReason": "end_turn"}})
    else:
        send({"id": message["id"], "error": {"code": -32601, "message": "not supported"}})
