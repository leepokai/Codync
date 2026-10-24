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
            assert {t["name"] for t in tools["result"]["tools"]} == {"list_bots", "message_bot"}
            roster = rpc(3, "tools/call", {"name": "list_bots", "arguments": {}})
            assert any(b["id"] == arguments["botId"] for b in json.loads(roster["result"]["content"][0]["text"])["bots"])
        response = rpc(4, "tools/call", {"name": tool, "arguments": arguments})
        mcp.stdin.close()
        return response["result"]


def team_call(target, message):
    return mcp_call("team", "message_bot", {"botId": target, "message": message})


def chain():
    """Hands the chain on to the bot in handoff.json; what the turn replies."""
    response = team_call(json.loads(pathlib.Path("handoff.json").read_text())["target"], "CHAIN")
    return "Chain stopped: " + response["content"][0]["text"] if response.get("isError") else "Chain forwarded"


REPLIES_END = "\n\n[Codync: these are bot replies"


def prompt(request):
    text = request["params"]["prompt"][0]["text"]
    with open("prompts.jsonl", "a") as log:
        log.write(json.dumps(text) + "\n")
    if REPLIES_END in text:
        # Replies to requests this bot sent: relay what they said (whatever words they carry),
        # or pass on a chain the peer handed back.
        parts = text.split(REPLIES_END, 1)[0].split("\n\n")
        relayed = [p.split(".]\n", 1)[1] if ".]\n" in p else "failed" for p in parts]
        reply = chain() if any("CHAIN" in r for r in relayed) else "Relayed: " + " | ".join(relayed)
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}})
        # A text update can reach sync before the host handles turn completion.
        # Keep that interval deterministic when the bounded chain stops.
        if reply.startswith("Chain stopped"):
            time.sleep(0.3)
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})
        return
    if text.startswith(("DELEGATE ", "DELEGATE_REPORT ")):
        target = text.split()[1]
        update({"sessionUpdate": "tool_call", "toolCallId": "delegate", "title": "Message reviewer", "status": "in_progress"})
        message = "REPORT" if text.startswith("DELEGATE_REPORT ") else "PERMISSION review the changes"
        response = team_call(target, message)
        assert not response.get("isError"), response
        result = json.loads(response["content"][0]["text"])
        assert result["status"] == "queued" and "reply" not in result
        update({"sessionUpdate": "tool_call_update", "toolCallId": "delegate", "status": "completed"})
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Message queued"}})
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})
        return
    if text.split("\n\n")[-1] == "REPORT":
        response = mcp_call("chat", "send_message", {"text": "Report for the user"})
        if "Your final reply goes back to" in text:
            assert response.get("isError"), response
            assert "requesting bot" in response["content"][0]["text"]
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
        reply = chain()
        if "Your final reply goes back to" in text:
            # A request turn messages no one: it hands the chain back in its reply.
            assert "answering another bot's request" in reply, reply
            reply = "CHAIN back"
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}})
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})
        return
    if text.split("\n\n")[-1].startswith("MANAGE "):
        tool, arguments = text.split("\n\n")[-1].split(" ", 2)[1:]
        response = mcp_call("manage", tool, json.loads(arguments))
        reply = ("Refused: " if response.get("isError") else "Done: ") + response["content"][0]["text"]
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}})
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})
        return
    if text.split("\n\n")[-1] == "OK":
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "(pass)"}})
        send({"id": request["id"], "result": {"stopReason": "end_turn"}})
        return
    if text.split("\n\n")[-1] == "THANKS":
        response = mcp_call("chat", "react", {"emoji": "❤️"})
        reply = "Held back a reaction" if response.get("isError") else "Reacted with a heart"
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}})
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
        if last.startswith("User:") and "PING" in last:
            reply = "@b over to you"
        if "REACT" in last:
            response = mcp_call("chat", "react", {"emoji": "👍", "to": "user"})
            assert not response.get("isError"), response
            reply = "(pass)"
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
