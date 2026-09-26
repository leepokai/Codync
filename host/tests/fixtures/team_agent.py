"""Deterministic ACP agent for delegation tests; no provider or network access."""

import json
import pathlib
import subprocess
import sys
import threading
import time

output_lock = threading.Lock()
cancel = threading.Event()
permission = threading.Event()
permission_reply = None
servers = []


def send(value):
    with output_lock:
        print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def update(value):
    send({"method": "session/update", "params": {"sessionId": "session", "update": value}})


def prompt(request):
    text = request["params"]["prompt"][0]["text"]
    with open("prompts.jsonl", "a") as log:
        log.write(json.dumps(text) + "\n")
    if text.startswith("DELEGATE "):
        target = text.split()[1]
        server = next(s for s in servers if s["name"] == "team")
        update({"sessionUpdate": "tool_call", "toolCallId": "delegate", "title": "Ask reviewer", "status": "in_progress"})
        with subprocess.Popen([server["command"], *server["args"]], stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, text=True) as mcp:
            def rpc(identifier, method, params):
                mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}) + "\n")
                mcp.stdin.flush()
                return json.loads(mcp.stdout.readline())

            initialized = rpc(1, "initialize", {"protocolVersion": "2025-06-18"})
            assert initialized["result"]["serverInfo"]["name"] == "codync-team"
            tools = rpc(2, "tools/list", {})
            assert {t["name"] for t in tools["result"]["tools"]} == {"list_bots", "ask_bot"}
            roster = rpc(3, "tools/call", {"name": "list_bots", "arguments": {}})
            assert any(b["id"] == target for b in json.loads(roster["result"]["content"][0]["text"])["bots"])
            response = rpc(4, "tools/call", {"name": "ask_bot", "arguments": {
                "botId": target, "message": "PERMISSION review the changes",
            }})
            assert not response["result"].get("isError"), response
            text = json.loads(response["result"]["content"][0]["text"])["reply"]
            mcp.stdin.close()
        update({"sessionUpdate": "tool_call_update", "toolCallId": "delegate", "status": "completed"})
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Team reply: " + text}})
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})
        return
    if "PERMISSION" in text:
        send({"id": "permission", "method": "session/request_permission", "params": {
            "sessionId": "session", "toolCall": {"toolCallId": "edit", "title": "Edit a file"},
            "options": [{"optionId": "allow", "name": "Allow", "kind": "allow_once"}],
        }})
        while not cancel.is_set() and not permission.wait(0.01):
            pass
    while "BLOCK" in text and not pathlib.Path("release").exists() and not cancel.wait(0.01):
        pass
    if text.startswith("[Group chat") or "\n[Group chat" in text:
        # A room turn: answer only when the user spoke last, otherwise pass.
        room = text.split("New messages in the room (oldest first):\n", 1)[-1].split("\n\n", 1)[0]
        last = room.strip().splitlines()[-1] if "New messages" in text else ""
        reply = "reply: group" if last.startswith("User:") else "(pass)"
        # Two chunks, apart: the streamed entry is rewritten before it's final.
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply[:3]}})
        time.sleep(0.4)
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply[3:]}})
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})
        return
    if cancel.is_set():
        send({"id": request["id"], "result": {"stopReason": "cancelled"}})
    elif "FAIL" in text:
        send({"id": request["id"], "error": {"code": -32000, "message": "fixture failure"}})
    else:
        if "EMPTY" not in text:
            update({"sessionUpdate": "agent_message_chunk", "content": {
                "type": "text", "text": "reply: " + text.split("\n\n")[-1],
            }})
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})


for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "initialize":
        send({"id": request["id"], "result": {"agentCapabilities": {
            "loadSession": True, "_meta": {"claudeCode": {}},
        }}})
    elif method in ("session/new", "session/load"):
        servers = request["params"]["mcpServers"]
        pathlib.Path("servers.json").write_text(json.dumps(request["params"]["mcpServers"]))
        send({"id": request["id"], "result": {"sessionId": "session"}})
    elif method == "session/prompt":
        cancel = threading.Event()
        permission = threading.Event()
        threading.Thread(target=prompt, args=(request,), daemon=True).start()
    elif method == "session/cancel":
        cancel.set()
    elif request.get("id") == "permission":
        permission_reply = request.get("result")
        permission.set()
    elif "id" in request:
        send({"id": request["id"], "result": {}})
