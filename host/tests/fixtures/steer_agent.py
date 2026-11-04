"""ACP agent whose turn waits: a steered message joins it, a cancel stops it, or it ends after `seconds`.

Args: `steer|late|limit|next|plain seconds [answer_delay]`. With `steer` it advertises `_session/steering` (top-level
`_meta`, as Claude's and Codex's adapters do) and takes a steered message into its running turn; with `late` it ends
that turn before answering; with `limit` that turn then fails the way Claude's adapter reports a usage limit; with
`next` it advertises steering too but always answers that the message needs a turn of its own. It answers a steer after
`answer_delay` seconds. Every request is logged to requests.jsonl in its folder.
"""

import json
import sys
import threading
import time

MODE = sys.argv[1]
SECONDS = float(sys.argv[2])
ANSWER_DELAY = float(sys.argv[3]) if len(sys.argv) > 3 else 0
SID = "s1"
LIMIT = {
    "code": -32603,
    "message": "Internal error: You've hit your limit · resets 3pm (Asia/Taipei)",
    "data": {"errorKind": "rate_limit"},
}
lock = threading.Lock()
turn = {"wake": None, "steered": None, "cancelled": False}


def send(value):
    with lock:
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
        sys.stdout.flush()


def say(text):
    update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}
    send({"method": "session/update", "params": {"sessionId": SID, "update": update}})


def run(rid):
    wake = threading.Event()
    turn.update(wake=wake, steered=None, cancelled=False)
    say("Working")
    wake.wait(SECONDS)
    turn["wake"] = None
    if turn["cancelled"]:
        send({"id": rid, "result": {"stopReason": "cancelled"}})
        return
    if MODE == "limit":
        send({"id": rid, "error": LIMIT})
        return
    say("Got: " + turn["steered"] if turn["steered"] else "Done")
    send({"id": rid, "result": {"stopReason": "end_turn"}})


def steer(message):
    time.sleep(ANSWER_DELAY)
    wake = turn["wake"]
    if MODE == "next" or wake is None:
        send({"id": message["id"], "result": {"outcome": "promptRequired", "reason": "noRunningTurn"}})
        return
    turn["steered"] = message["params"]["prompt"][0]["text"]
    if MODE == "late":
        wake.set()
        time.sleep(0.5)
    send({"id": message["id"], "result": {"outcome": "injected"}})
    if MODE == "limit":
        time.sleep(0.3)
    wake.set()


init = {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}
if MODE != "plain":
    init["_meta"] = {"steering": {"supported": True}}
with open("requests.jsonl", "a") as log:
    for line in sys.stdin:
        message = json.loads(line)
        log.write(line if line.endswith("\n") else line + "\n")
        log.flush()
        method = message.get("method")
        if method == "initialize":
            send({"id": message["id"], "result": init})
        elif method == "session/new":
            send({"id": message["id"], "result": {"sessionId": SID}})
        elif method == "session/prompt":
            threading.Thread(target=run, args=(message["id"],)).start()
        elif method == "_session/steering":
            threading.Thread(target=steer, args=(message,)).start()
        elif method == "session/cancel" and turn["wake"] is not None:
            turn["cancelled"] = True
            turn["wake"].set()
        elif "id" in message and method:
            send({"id": message["id"], "result": {}})
