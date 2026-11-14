"""Deterministic ACP agent for task tests: a bot that starts, tells and stops tasks, and the task itself.

Argument `no-fork`: the agent doesn't advertise `session/fork`. A `no-start` file in its folder: it doesn't start.
"""

import json
import os
import pathlib
import re
import subprocess
import sys
import threading

sys.stdin.reconfigure(encoding="utf-8")
sys.stdout.reconfigure(encoding="utf-8")

if pathlib.Path("no-start").exists():
    sys.exit(1)

CAN_FORK = "no-fork" not in sys.argv
output_lock = threading.Lock()
cancel = threading.Event()
permission = threading.Event()
servers = []
forked = set()
count = 0


def send(value):
    with output_lock:
        print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def log(name, value):
    with open(name, "a", encoding="utf-8") as f:
        f.write(json.dumps(value) + "\n")


def new_session():
    global count
    count += 1
    return f"s{os.getpid()}-{count}"


def chat_tool(tool, arguments):
    server = next(s for s in servers if s["name"] == "chat")
    with subprocess.Popen([server["command"], *server["args"]], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, encoding="utf-8") as mcp:
        def rpc(identifier, method, params):
            mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}) + "\n")
            mcp.stdin.flush()
            return json.loads(mcp.stdout.readline())

        rpc(1, "initialize", {"protocolVersion": "2025-06-18"})
        listed = {t["name"] for t in rpc(2, "tools/list", {})["result"]["tools"]}
        assert {"start_task", "tell_task", "task_status", "stop_task"} <= listed, listed
        response = rpc(3, "tools/call", {"name": tool, "arguments": arguments})
        mcp.stdin.close()
        return response["result"]


def said(response, done):
    return ("Refused: " if response.get("isError") else done) + response["content"][0]["text"]


def answer(text, session):
    """What this turn replies."""
    if "[Codync: your task" in text:
        report = text.split("\n\n[Codync: these are your tasks' reports", 1)[0]
        # A finished task's report follows the bracket; one that didn't finish is all in it.
        return "Relayed: " + (report.split("]", 1)[1].strip() or report)
    if "[From the chat" in text:
        return "Told: " + text.split("[From the chat, passed on by you there] ", 1)[1].split("\n\n", 1)[0]
    if "[Task]" in text:
        notes = ["forked" if session in forked else "briefed",
                 "saw the chat" if "[The chat you come from, so far]" in text else "no chat"]
        if "ASK" in text:
            notes.append(said(chat_tool("request_decision", {"question": "Which?", "options": ["A", "B"]}), "Asked: ")[:60])
        if "NEST" in text:
            notes.append(said(chat_tool("start_task", {"title": "Inner", "brief": "x"}), "Nested: ")[:60])
        if "PERMISSION" in text:
            send({"id": "permission", "method": "session/request_permission", "params": {
                "sessionId": session, "toolCall": {"toolCallId": "edit", "title": "Run the tests"},
                "options": [{"optionId": "allow", "name": "Allow", "kind": "allow_once"}],
            }})
            while not cancel.is_set() and not permission.wait(0.01):
                pass
            notes.append("approved")
        while "HOLD" in text and not pathlib.Path("release").exists() and not cancel.wait(0.01):
            pass
        return "Report: " + "; ".join(notes)
    command = re.search(r"^(START|TELL|STATUS|STOPTASK)\b ?(.*)$", text, re.M)
    if not command:
        return "reply: " + text.strip().splitlines()[-1]
    name, rest = command.groups()
    if name == "START":
        title, brief, *flags = rest.split("|")
        arguments = {"title": title, "brief": brief}
        if "parallel" in flags:
            arguments["parallel"] = True
        started = said(chat_tool("start_task", arguments), "Started: ")
        while "hold" in flags and not cancel.wait(0.01):
            pass
        return started
    if name == "TELL":
        return said(chat_tool("tell_task", {"message": rest}), "Passed on: ")
    if name == "STATUS":
        return said(chat_tool("task_status", {}), "Status: ")
    return said(chat_tool("stop_task", {}), "Stopped: ")


def prompt(request):
    session = request["params"]["sessionId"]
    text = request["params"]["prompt"][0]["text"]
    log("prompts.jsonl", {"session": session, "text": text})
    reply = answer(text, session)
    if cancel.is_set():
        send({"id": request["id"], "result": {"stopReason": "cancelled"}})
        return
    send({"method": "session/update", "params": {"sessionId": session, "update": {
        "sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}}}})
    send({"id": request["id"], "result": {"stopReason": "end_turn"}})


for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    params = message.get("params") or {}
    if method == "initialize":
        capabilities = {"close": {}}
        if CAN_FORK:
            capabilities["fork"] = {}
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {
            "loadSession": True, "sessionCapabilities": capabilities}}})
    elif method in ("session/new", "session/fork", "session/load"):
        servers = params.get("mcpServers", servers)
        session = params["sessionId"] if method == "session/load" else new_session()
        if method == "session/fork":
            forked.add(session)
            log("forks.jsonl", {"from": params["sessionId"], "to": session})
        send({"id": message["id"], "result": {} if method == "session/load" else {"sessionId": session}})
    elif method == "session/close":
        send({"id": message["id"], "result": {}})
    elif method == "session/prompt":
        cancel.clear()
        permission.clear()
        threading.Thread(target=prompt, args=(message,), daemon=True).start()
    elif method == "session/cancel":
        cancel.set()
    elif message.get("id") == "permission" and method is None:
        permission.set()
    elif "id" in message and method:
        send({"id": message["id"], "error": {"code": -32601, "message": "not supported"}})
