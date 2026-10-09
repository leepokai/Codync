"""Persistent ACP sessions, independently observable tool trees and load failures."""

import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import threading
import time
import uuid

if len(sys.argv) > 1:
    # TCP EOF is an exit acknowledgement even when the host uses SIGKILL.
    config = json.loads(Path("fixture.json").read_text())
    witness = socket.create_connection(("127.0.0.1", config["witnessPort"]))
    witness.sendall((json.dumps({"pid": os.getpid()}) + "\n").encode())
    leaf = None
    if sys.argv[1] == "worker":
        leaf = subprocess.Popen([sys.executable, "-u", __file__, "leaf"], stdin=subprocess.DEVNULL,
                                stdout=sys.stdout, stderr=sys.stderr)

    def stop(*_):
        if leaf is not None:
            leaf.terminate()
            leaf.wait(timeout=5)
        sys.exit(0)

    signal.signal(signal.SIGTERM, stop)
    if leaf is not None:
        sys.stdin.readline()
        stop()
    else:
        while True:
            time.sleep(1)

config = json.loads(Path("fixture.json").read_text())
generation = str(uuid.uuid4())
saved_file = Path("sessions.json")
saved = json.loads(saved_file.read_text()) if saved_file.exists() else {}
live = {}
workers = {}
lock = threading.RLock()
cancel = threading.Event()
permission = threading.Event()


def send(value):
    with lock:
        print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def event(kind, sid=None, **fields):
    with lock, Path("events.jsonl").open("a") as output:
        output.write(json.dumps({"event": kind, "generation": generation,
                                 "session": sid, "live": len(live), **fields}) + "\n")


def persist():
    temp = saved_file.with_suffix(".tmp")
    temp.write_text(json.dumps(saved))
    temp.replace(saved_file)


def allocate(sid, params):
    # Deliberately leak resources on repeat load unless the client closes first.
    if sid in workers:
        event("duplicate_allocation", sid)
    workers.setdefault(sid, []).append(subprocess.Popen([sys.executable, __file__, "worker"], stdin=subprocess.PIPE, text=True))
    live[sid] = params.get("_meta", {})
    event("allocated", sid, memoryLane=next((item["value"] for server in params.get("mcpServers", [])
          for item in server.get("env", []) if item["name"] == "CODYNC_MEMORY_LANE"), None))


def prompt(message):
    sid = message["params"]["sessionId"]
    text = message["params"]["prompt"][0]["text"]
    if "BLOCK" in text:
        event("blocked", sid)
        while not Path("release").exists() and not cancel.wait(.01):
            pass
    if "PERMISSION" in text:
        permission.clear()
        send({"id": 900, "method": "session/request_permission", "params": {
            "sessionId": sid, "toolCall": {"toolCallId": "permission", "title": "Test approval", "kind": "execute"},
            "options": [{"optionId": "allow", "name": "Allow", "kind": "allow_once"}]}})
        event("permission", sid)
        permission.wait()
    if "COMPACT" in text:
        send({"method": "session/update", "params": {"sessionId": sid, "update": {
            "sessionUpdate": "compaction_update", "status": "completed", "compactionId": str(uuid.uuid4())}}})
    with lock:
        saved[sid].append(text)
        persist()
        summary = f"SID={sid} COUNT={len(saved[sid])} {text.splitlines()[-1]}"
    send({"method": "session/update", "params": {"sessionId": sid, "update": {
        "sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": summary}}}})
    event("prompted", sid, profile="<bot-profile>" in text, threadIntro="[Thread]" in text,
          forkIntro="starts from everything said so far" in text)
    send({"id": message["id"], "result": {"stopReason": "cancelled" if cancel.is_set() else "end_turn"}})


event("started")
for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    params = message.get("params", {})
    if method is None:
        if message.get("id") == 900:
            permission.set()
        continue
    rid = message.get("id")
    if method == "initialize":
        caps = {"loadSession": config.get("load", True), "sessionCapabilities": {}}
        if config.get("close", False):
            caps["sessionCapabilities"]["close"] = {}
        if config.get("fork", False):
            caps["sessionCapabilities"]["fork"] = {}
        if config.get("claude", False):
            caps["_meta"] = {"claudeCode": {}}
        send({"id": rid, "result": {"protocolVersion": 1, "agentCapabilities": caps}})
    elif method == "session/new":
        sid = str(uuid.uuid4())
        saved[sid] = []
        persist()
        allocate(sid, params)
        if Path("hang-new").exists():
            continue
        if Path("fail-new").exists():
            send({"id": rid, "error": {"code": -32001, "message": "allocation failed after tools started"}})
        else:
            send({"id": rid, "result": {"sessionId": sid}})
    elif method == "session/fork":
        parent = params["sessionId"]
        if parent not in live:
            send({"id": rid, "error": {"code": -32602, "message": "parent must be live"}})
        else:
            sid = str(uuid.uuid4())
            saved[sid] = saved[parent].copy()
            persist()
            allocate(sid, params)
            returned = {"empty": "", "parent": parent}.get(config.get("forkResponse"), sid)
            send({"id": rid, "result": {"sessionId": returned}})
    elif method == "session/load":
        sid = params["sessionId"]
        if Path("fail-load").exists():
            event("load_failed", sid)
            send({"id": rid, "error": {"code": -32001, "message": "transient load failure"}})
        elif sid not in saved:
            send({"id": rid, "error": {"code": -32602, "message": "missing saved session"}})
        else:
            allocate(sid, params)
            send({"method": "session/update", "params": {"sessionId": sid, "update": {
                "sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "REPLAY"}}}})
            event("loaded", sid)
            send({"id": rid, "result": {}})
    elif method == "session/close":
        event("closing", params["sessionId"])
        if config.get("closeHangs", False):
            continue
        if config.get("closeFails", False):
            send({"id": rid, "error": {"code": -32001, "message": "close failed"}})
        else:
            sid = params["sessionId"]
            for worker in workers.pop(sid, []):
                worker.stdin.write("stop\n")
                worker.stdin.flush()
                worker.wait(timeout=5)
            live.pop(sid, None)
            event("closed", sid)
            send({"id": rid, "result": {}})
    elif method == "session/prompt":
        cancel.clear()
        threading.Thread(target=prompt, args=(message,)).start()
    elif method == "session/cancel":
        cancel.set()
        permission.set()
    elif method in ("session/set_model", "session/set_config_option"):
        if Path("hang-model").exists():
            event("model_pending", params["sessionId"])
        elif Path("fail-model").exists():
            event("model_failed", params["sessionId"])
            send({"id": rid, "error": {"code": -32001, "message": "transient model selection failure"}})
        else:
            event("model", params["sessionId"])
            send({"id": rid, "result": {}})
