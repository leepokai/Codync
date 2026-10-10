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


def mcp_call(server_name, tool, arguments):
    server = next(s for s in servers if s["name"] == server_name)
    with subprocess.Popen([server["command"], *server["args"]], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, text=True) as mcp:
        def rpc(identifier, method, params):
            mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}) + "\n")
            mcp.stdin.flush()
            return json.loads(mcp.stdout.readline())

        initialized = rpc(1, "initialize", {"protocolVersion": "2025-06-18"})
        assert initialized["result"]["serverInfo"]["name"] == "codync-" + server_name
        tools = rpc(2, "tools/list", {})
        assert any(t["name"] == tool for t in tools["result"]["tools"])
        if server_name == "team":
            assert {t["name"] for t in tools["result"]["tools"]} == {"list_bots", "ask_bot", "message_bot"}
            roster = rpc(3, "tools/call", {"name": "list_bots", "arguments": {}})
            assert any(b["id"] == arguments["botId"] for b in json.loads(roster["result"]["content"][0]["text"])["bots"])
        response = rpc(4, "tools/call", {"name": tool, "arguments": arguments})
        mcp.stdin.close()
        return response["result"]


def team_call(target, tool, message):
    return mcp_call("team", tool, {"botId": target, "message": message})


def prompt(request):
    text = request["params"]["prompt"][0]["text"]
    with open("prompts.jsonl", "a") as log:
        log.write(json.dumps(text) + "\n")
    if text.startswith(("DELEGATE ", "MESSAGE ", "ASK_REPORT ", "MESSAGE_REPORT ")):
        independent = text.startswith(("MESSAGE ", "MESSAGE_REPORT "))
        target = text.split()[1]
        update({"sessionUpdate": "tool_call", "toolCallId": "delegate", "title": "Ask reviewer", "status": "in_progress"})
        message = "REPORT" if "_REPORT " in text else "PERMISSION review the changes"
        response = team_call(target, "message_bot" if independent else "ask_bot", message)
        assert not response.get("isError"), response
        result = json.loads(response["content"][0]["text"])
        if independent:
            assert result["status"] == "queued" and "reply" not in result
            text = "Message queued"
        else:
            text = "Team reply: " + result["reply"]
        update({"sessionUpdate": "tool_call_update", "toolCallId": "delegate", "status": "completed"})
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}})
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})
        return
    if text.split("\n\n")[-1] == "REPORT":
        response = mcp_call("chat", "send_message", {"text": "Report for the user"})
        if "requests your help" in text:
            assert response.get("isError"), response
            assert "requesting bot" in response["content"][0]["text"]
            assert "Do not use send_message or message_bot" in text
            reply = "Answer for the requesting bot"
        else:
            assert not response.get("isError"), response
            response = mcp_call("chat", "send_message", {"text": "Second report for the user"})
            assert not response.get("isError"), response
            reply = "Private completion trace"
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}})
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})
        return
    if text.split("\n\n")[-1] == "CHAIN":
        route = json.loads(pathlib.Path("handoff.json").read_text())
        response = team_call(route["target"], route["tool"], "CHAIN")
        if response.get("isError"):
            reply = "Chain stopped: " + response["content"][0]["text"]
        else:
            result = json.loads(response["content"][0]["text"])
            reply = result.get("reply", "Chain forwarded")
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}})
        # A text update can reach sync before the host handles turn completion.
        # Keep that interval deterministic when the bounded chain stops.
        if response.get("isError"):
            time.sleep(0.3)
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
        while "BLOCK" in text and not pathlib.Path("release").exists() and not cancel.wait(0.01):
            pass
        if cancel.is_set():
            send({"id": request["id"], "result": {"stopReason": "cancelled"}})
            return
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
    elif "REFUSE" in text:
        send({"id": request["id"], "result": {"stopReason": "refusal"}})
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
        with open("sessions.jsonl", "a") as log:
            log.write(json.dumps(request) + "\n")
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
