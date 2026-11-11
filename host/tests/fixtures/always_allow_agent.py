"""ACP agent that, like MiniMax Code, asks before every action even after the user chose Always allow. Each session
it opens gets a new id (s1, s2…). Each prompt asks permission once: to run a command, or to edit a file when the
prompt says EDIT (upper case: the host adds context to a prompt, where a lower-case word may appear). Its reply
names the session and the option it was answered with."""

import json
import sys
import threading

sessions = 0
asked = 100
waiting = {}  # request id -> (event, answer holder)


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def turn(rid, session, text):
    global asked
    asked += 1
    kind, title = ("edit", "Edit a.txt") if "EDIT" in text else ("execute", "Run a command")
    tool = {"toolCallId": f"t{asked}", "title": title, "kind": kind}
    options = [{"optionId": "once", "name": "Allow", "kind": "allow_once"},
               {"optionId": "always", "name": "Always allow", "kind": "allow_always"},
               {"optionId": "no", "name": "Reject", "kind": "reject_once"}]
    done, holder = threading.Event(), {}
    waiting[asked] = (done, holder)
    send({"id": asked, "method": "session/request_permission",
          "params": {"sessionId": session, "toolCall": tool, "options": options}})
    done.wait()
    outcome = holder.get("outcome", {})
    chosen = outcome.get("optionId") if outcome.get("outcome") == "selected" else "cancelled"
    update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"{session} {chosen}"}}
    send({"method": "session/update", "params": {"sessionId": session, "update": update}})
    send({"id": rid, "result": {"stopReason": "end_turn"}})


for line in sys.stdin:
    message = json.loads(line)
    method, rid = message.get("method"), message.get("id")
    if method is None:
        done, holder = waiting.pop(rid, (None, None))
        if done:
            holder.update(message.get("result", {}))
            done.set()
        continue
    params = message.get("params", {})
    if method == "initialize":
        send({"id": rid, "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        sessions += 1
        send({"id": rid, "result": {"sessionId": f"s{sessions}"}})
    elif method == "session/prompt":
        text = " ".join(b.get("text", "") for b in params.get("prompt", []))
        threading.Thread(target=turn, args=(rid, params.get("sessionId"), text)).start()
    elif method == "session/cancel":
        for done, holder in list(waiting.values()):
            holder["outcome"] = {"outcome": "cancelled"}
            done.set()
    elif rid is not None:
        send({"id": rid, "result": {}})
