"""Persistent deterministic ACP sessions for routine restart and queue tests."""
import json
import os
import pathlib
import re
import subprocess
import sys
import threading
import time
import uuid

lock = threading.RLock()
sessions_file = pathlib.Path("sessions.json")
sessions = json.loads(sessions_file.read_text()) if sessions_file.exists() else {}
cancel = threading.Event()
servers = []


def send(value):
    with lock:
        print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def log(event, sid, text):
    with lock:
        with pathlib.Path("events.jsonl").open("a") as output:
            output.write(json.dumps({"event": event, "session": sid, "text": text}) + "\n")


def answer_late(answer):
    log("late", answer["result"]["sessionId"], "")
    send(answer)


def prompt(request, stop):
    sid = request["params"]["sessionId"]
    text = request["params"]["prompt"][0]["text"]
    resumed = "[Codync restarted" in text
    with lock:
        if resumed:
            text = sessions[sid]
        else:
            sessions[sid] = text
            temp = sessions_file.with_suffix(".tmp")
            temp.write_text(json.dumps(sessions))
            temp.replace(sessions_file)
    log("resumed" if resumed else "started", sid, text)
    if "VERIFY_ROUTINE_TOOLS" in text and "Scheduled routine:" not in text:
        try:
            verify_routine_tools()
            log("tools_verified", sid, "Routine MCP lifecycle")
        except Exception as error:
            log("tools_failed", sid, repr(error))
            send({"id": request["id"], "error": {"code": -32000, "message": str(error)}})
            return
    block = re.search(r"BLOCK\[([^]]+)\]", text)
    if block:
        while not pathlib.Path("release-" + block[1]).exists():
            if stop.wait(0.01) and "IGNORE_CANCEL" not in text:
                send({"id": request["id"], "result": {"stopReason": "cancelled"}})
                return
    fail = re.search(r"FAIL_ONCE\[([^]]+)\]", text)
    if fail and not pathlib.Path("failed-" + fail[1]).exists():
        pathlib.Path("failed-" + fail[1]).touch()
        if "ACT_FIRST" in text:
            # Changes something before failing, so the run may have done part of its work.
            send({"method": "session/update", "params": {"sessionId": sid, "update": {
                "sessionUpdate": "tool_call", "toolCallId": "act", "title": "Send the report",
                "kind": "edit", "status": "completed"}}})
        log("failed", sid, text)
        send({"id": request["id"], "error": {"code": -32000, "message": "transient fixture failure"}})
        return
    log("finished", sid, text)
    reply = "(pass)" if "SILENT" in text else "Routine completed successfully."
    send({"method": "session/update", "params": {"sessionId": sid, "update": {
        "sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}}}})
    send({"id": request["id"], "result": {"stopReason": "end_turn"}})


def verify_routine_tools():
    server = next(s for s in servers if s["name"] == "routines")
    with subprocess.Popen([server["command"], *server["args"]], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, text=True) as mcp:
        identifier = 0

        def rpc(method, params):
            nonlocal identifier
            identifier += 1
            mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identifier,
                                       "method": method, "params": params}) + "\n")
            mcp.stdin.flush()
            response = json.loads(mcp.stdout.readline())
            assert "error" not in response, response
            return response["result"]

        def call(name, arguments):
            result = rpc("tools/call", {"name": name, "arguments": arguments})
            assert not result.get("isError"), result
            return json.loads(result["content"][0]["text"])

        init = rpc("initialize", {"protocolVersion": "2025-06-18"})
        assert init["serverInfo"]["name"] == "codync-routines"
        names = {t["name"] for t in rpc("tools/list", {})["tools"]}
        assert names == {"list_routines", "save_routine", "set_routine_enabled",
                         "delete_routine", "run_routine", "routine_webhook"}, names
        assert not call("list_routines", {})["routines"]
        invalid = rpc("tools/call", {"name": "save_routine", "arguments": {
            "name": "Invalid", "instruction": "Check", "triggers": [{"type": "interval", "seconds": 0}]}})
        assert invalid.get("isError"), invalid
        body = {"name": "Created through MCP", "instruction": "MCP_TASK_RESULT",
                "triggers": [{"type": "webhook"}], "enabled": False}
        routine = call("save_routine", body)["routine"]
        identifier_value = routine["id"]
        assert routine["enabled"] is False
        assert "webhookKey" not in routine
        credentials = call("routine_webhook", {"id": identifier_value})
        assert credentials["url"] is None  # the test host runs with the cloud off
        assert credentials["localUrl"].endswith("/hooks/routines/" + identifier_value)
        assert len(credentials["key"]) >= 32
        rotated = call("routine_webhook", {"id": identifier_value, "rotate": True})
        assert rotated["key"] != credentials["key"] and len(rotated["key"]) >= 32
        body.update({"id": identifier_value, "name": "Edited through MCP"})
        call("save_routine", body)
        call("set_routine_enabled", {"id": identifier_value, "enabled": True})
        call("set_routine_enabled", {"id": identifier_value, "enabled": False})
        # A temporary second routine proves deletion is routed through MCP too.
        extra = call("save_routine", {**body, "id": None, "name": "Temporary"})["routine"]
        call("delete_routine", {"id": extra["id"]})
        listed = call("list_routines", {})["routines"]
        assert len(listed) == 1 and listed[0]["name"] == "Edited through MCP"
        # A routine that isn't this bot's doesn't run: the error says to ask its bot.
        foreign = rpc("tools/call", {"name": "run_routine", "arguments": {"id": "another-bots-routine"}})
        assert foreign.get("isError") and "message_bot" in foreign["content"][0]["text"], foreign
        call("run_routine", {"id": identifier_value})
        mcp.stdin.close()


for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "initialize":
        log("initialize", "", str(os.getpid()))
        resumable = not pathlib.Path("no-load").exists() and "--no-load" not in sys.argv
        capabilities = {"loadSession": resumable, "sessionCapabilities": {"close": {}}}
        send({"id": request["id"], "result": {"agentCapabilities": capabilities}})
    elif method == "session/new":
        servers = request["params"]["mcpServers"]
        answer = {"id": request["id"], "result": {"sessionId": str(uuid.uuid4())}}
        if "--slow-new" in sys.argv and not pathlib.Path("slowed").exists():
            # The first session ever opens 4 s late (its tools starting), and is logged as it does, unless the agent
            # is stopped first.
            pathlib.Path("slowed").touch()
            late = threading.Timer(4, answer_late, args=(answer,))
            late.daemon = True
            late.start()
        elif "--hang-new" not in sys.argv:  # an agent stuck starting never answers
            send(answer)
    elif method == "session/load":
        sid = request["params"]["sessionId"]
        if sid in sessions:
            log("loaded", sid, "")
            send({"id": request["id"], "result": {"sessionId": sid}})
        else:
            send({"id": request["id"], "error": {"code": -32000, "message": "unknown session"}})
    elif method == "session/prompt":
        cancel = threading.Event()
        threading.Thread(target=prompt, args=(request, cancel), daemon=True).start()
    elif method == "session/cancel":
        cancel.set()
    elif method == "session/close":
        log("closed", request["params"]["sessionId"], "")
        send({"id": request["id"], "result": {}})
    elif "id" in request:
        send({"id": request["id"], "result": {}})
