"""ACP agent whose turn waits: a steered message joins it, a cancel stops it, or it ends after `seconds`.

Args: `steer|sends|voice|voice-wav|late|sends-late|limit|next|refuse|lingers|plain seconds [answer_delay]`. With `steer` it
advertises `_session/steering` (top-level `_meta`, as Claude's and Codex's adapters do) and takes a steered message into
its running turn; with `sends` it also calls `send_message` before it takes the message and again after, before its
last text; with `voice` it sends a voice note (`send_file` with text) before it takes the message and ends with no more
text (`voice-wav`: the same with four seconds of real, silent WAV a player can load); with `late` it ends that turn before answering; `sends-late` does both, calling `send_message` once; with `limit`
that turn then fails the way Claude's adapter reports a usage limit; with `next` it advertises steering too but always
answers that the message needs a turn of its own; with `refuse` it answers a steer with an error; with `lingers` it
takes two seconds to wind down after a cancel. It answers a steer after `answer_delay` seconds, so a turn shorter than
that ends first. Every request is logged to requests.jsonl in its folder.
"""

import json
import subprocess
import wave
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
servers = []


def send(value):
    with lock:
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
        sys.stdout.flush()


def say(text):
    update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}
    send({"method": "session/update", "params": {"sessionId": SID, "update": update}})


def chat_tool(name, arguments):
    server = next(s for s in servers if s["name"] == "chat")
    calls = [
        ("initialize", {"protocolVersion": "2025-06-18"}),
        ("tools/call", {"name": name, "arguments": arguments}),
    ]
    with subprocess.Popen([server["command"], *server["args"]], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True) as mcp:
        for identifier, (method, params) in enumerate(calls):
            mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}) + "\n")
            mcp.stdin.flush()
            mcp.stdout.readline()


def send_message(text):
    chat_tool("send_message", {"text": text})


def run(rid):
    wake = threading.Event()
    turn.update(wake=wake, steered=None, cancelled=False)
    say("Working")
    if MODE in ("sends", "sends-late"):
        send_message("Before it joined")
    if MODE == "voice":
        with open("note.m4a", "wb") as note:
            note.write(b"\0\0\0\x18ftypM4A voice")
        chat_tool("send_file", {"path": "note.m4a", "text": "Listen before it joined"})
    if MODE == "voice-wav":
        with wave.open("note.wav", "wb") as note:
            note.setnchannels(1)
            note.setsampwidth(2)
            note.setframerate(8000)
            note.writeframes(b"\0\0" * 8000 * 4)
        chat_tool("send_file", {"path": "note.wav", "text": "Listen before it joined"})
    wake.wait(SECONDS)
    turn["wake"] = None
    if MODE == "sends" and turn["steered"]:
        say("About to send")
        send_message("After it joined")
    if turn["cancelled"]:
        if MODE == "lingers":
            time.sleep(2)
        send({"id": rid, "result": {"stopReason": "cancelled"}})
        return
    if MODE == "limit":
        send({"id": rid, "error": LIMIT})
        return
    if MODE not in ("voice", "voice-wav"):
        say("Got: " + turn["steered"] if turn["steered"] else "Done")
    send({"id": rid, "result": {"stopReason": "end_turn"}})


def steer(message):
    wake = turn["wake"]
    time.sleep(ANSWER_DELAY)
    if MODE == "refuse":
        send({"id": message["id"], "error": {"code": -32603, "message": "Can't take that now"}})
        return
    if MODE == "next" or wake is None:
        send({"id": message["id"], "result": {"outcome": "promptRequired", "reason": "noRunningTurn"}})
        return
    turn["steered"] = message["params"]["prompt"][0]["text"]
    if MODE in ("late", "sends-late"):
        wake.set()
        time.sleep(0.5)
    send({"id": message["id"], "result": {"outcome": "injected"}})
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
            servers[:] = message["params"]["mcpServers"]
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
