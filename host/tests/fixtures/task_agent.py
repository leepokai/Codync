"""Deterministic ACP agent for the tests of a bot's box: a bot that adds and steers work, and the task that does it.

In the chat: `ADD title|brief[|hold][|after=<title>]`, `STEER how|title[|brief, or the title that gives way]`,
`STATUS`, `MESSAGE <bot id>|<request>` (asks another bot). In a task, the brief says what it does: `HOLD:<name>`
works until a `release-<name>` file is in its folder, `PERMISSION` asks for an approval, `ASK` asks the user to
choose, `SHARE` sends a file (`QUOTE`: quoting the chat message that added the item), `ARTIFACT` publishes one,
`NEST` tries what a task may not, `REMEMBER` adds a line to `MEMORY.md` with `edit_memory` and `FORGET` tries to take
one out (both once it is released), `TOUCH` writes `made.txt` in its folder.

A routine run and another bot's request read the same words in the routine's instruction or the request, and also
`SEND` (a message to the user), `PROGRESS` (saves the routine's progress note), `FOLLOW` (sets a follow-up) and
`PASS` (nothing to tell).

What the user writes to a task's thread is a turn of its own: it answers `Thread <their words>: <notes>`, where the
notes say whether the session is the one that did the work (`same session`) or a new one told of it (`picked up`),
and the words may hold `HOLD:<name>`, `NEST` and `ASK` as a brief does.

Arguments: `no-fork`: the agent doesn't advertise `session/fork`. `no-load`: nor `session/load`, so a session can't
be picked up again. `steer`: it takes a message into its running turn (`_session/steering`), and the turn's reply
ends with `joined <the message>`. A `no-start` file in its folder: it doesn't start.
A `signedout` file: every prompt fails asking to sign in (ACP's -32000). `task-signedout` / `task-limit`: a task's
turn does its work and then fails, asking to sign in or the way Claude's adapter reports a usage limit. A brief that
says `SIGNIN` does that once: its turn leaves a `sign-in-asked` file and fails asking to sign in, and the message,
sent again, goes through.
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
CAN_LOAD = "no-load" not in sys.argv
CAN_STEER = "steer" in sys.argv
# What the user's messages steered into the running turn said.
joined = []
SIGNED_OUT = {"code": -32000, "message": "Authentication required"}
LIMIT = {
    "code": -32603,
    "message": "Internal error: You've hit your limit · resets 3pm (Asia/Taipei)",
    "data": {"errorKind": "rate_limit"},
}
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
    return server_tool("chat", tool, arguments)


def server_tool(name, tool, arguments):
    server = next(s for s in servers if s["name"] == name)
    with subprocess.Popen([server["command"], *server["args"]], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, encoding="utf-8") as mcp:
        def rpc(identifier, method, params):
            mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}) + "\n")
            mcp.stdin.flush()
            return json.loads(mcp.stdout.readline())

        rpc(1, "initialize", {"protocolVersion": "2025-06-18"})
        listed = {t["name"] for t in rpc(2, "tools/list", {})["result"]["tools"]}
        assert name != "chat" or {"add_work", "steer_work", "work_status"} <= listed, listed
        response = rpc(3, "tools/call", {"name": tool, "arguments": arguments})
        mcp.stdin.close()
        return response["result"]


def said(response, done):
    return ("Refused: " if response.get("isError") else done) + response["content"][0]["text"]


def item_id(title):
    """The id of the bot's item called `title`."""
    work = json.loads(chat_tool("work_status", {})["content"][0]["text"])["work"]
    return next(w["itemId"] for w in work if w["title"] == title)


def relayed(text):
    """What the chat says of the reports in `text`: each item's report, or why it didn't finish."""
    reports = text.split("\n\n[Codync: these are reports", 1)[0].removeprefix("[Codync: your work ")
    parts = [part.partition("]") for part in reports.split("\n\n[Codync: your work ")]
    return "Relayed: " + " | ".join(body.strip() or head for head, _, body in parts)


def hold(text):
    """Works until its `release-<name>` file is there, or it is stopped."""
    name = re.search(r"HOLD:(\w+)", text)
    while name and not pathlib.Path("release-" + name.group(1)).exists() and not cancel.wait(0.01):
        pass


def ask():
    return said(chat_tool("request_decision", {"question": "Which?", "options": ["A", "B"]}), "Asked: ")[:90]


def elsewhere(kind, text):
    """A routine run's turn, or one that answers another bot's request: what its words say it does."""
    notes = []
    if "ASK" in text:
        notes.append(ask())
    if "SEND" in text:
        notes.append(said(chat_tool("send_message", {"text": "hi"}), "Sent: ")[:70])
    if "NEST" in text:
        notes.append(said(chat_tool("add_work", {"title": "Inner", "brief": "x"}), "Nested: ")[:60])
    routine = re.search(r"save_progress with id (\S+)", text)
    if "PROGRESS" in text and routine:
        arguments = {"id": routine.group(1), "content": "handled 1"}
        notes.append(said(server_tool("routines", "save_progress", arguments), "Progress: ")[:40])
    if "FOLLOW" in text:
        arguments = {"minutes": 5, "note": "look again"}
        notes.append(said(server_tool("routines", "follow_up", arguments), "Follow-up: ")[:60])
    hold(text)
    return "(pass)" if "PASS" in text else f"{kind} done: " + "; ".join(notes)


def work(text, session):
    """A task's turn on its item."""
    lead, rest = text.split("[Task]", 1)[1].split("\n\n", 1)
    title = lead.split('"')[1]
    brief = rest.split("\n\n[", 1)[0]
    notes = ["next" if lead.startswith(" Next") else "forked" if session in forked else "briefed",
             "saw the chat" if "[The chat you come from, so far]" in text else "no chat"]
    if "set aside" in rest:
        notes.append("again")
    if "ASK" in brief:
        notes.append(ask())
    if "SHARE" in brief:
        pathlib.Path("shared.txt").write_text("from a task", encoding="utf-8")
        quoted = re.search(r"QUOTE:(\S+)", brief)
        arguments = {"path": "shared.txt", **({"reply_to": quoted.group(1)} if quoted else {})}
        notes.append(said(chat_tool("send_file", arguments), "Shared: ")[:60])
    if "ARTIFACT" in brief:
        pathlib.Path("page.html").write_text("<h1>Page</h1>", encoding="utf-8")
        notes.append(said(chat_tool("publish_artifact", {"path": "page.html", "title": "Page"}), "Published: ")[:40])
    if "NEST" in brief:
        notes.append(said(chat_tool("add_work", {"title": "Inner", "brief": "x"}), "Nested: ")[:60])
    if "TOUCH" in brief:
        pathlib.Path("made.txt").write_text("made by a task", encoding="utf-8")
    if "PERMISSION" in brief:
        send({"id": "permission", "method": "session/request_permission", "params": {
            "sessionId": session, "toolCall": {"toolCallId": "edit", "title": "Run the tests"},
            "options": [{"optionId": "allow", "name": "Allow", "kind": "allow_once"}],
        }})
        while not cancel.is_set() and not permission.wait(0.01):
            pass
        notes.append("approved")
    hold(brief)
    if "REMEMBER" in brief:
        arguments = {"file": "MEMORY.md", "append": "- Works at night"}
        notes.append(said(server_tool("memory", "edit_memory", arguments), "Remembered: ")[:40])
    if "FORGET" in brief:
        arguments = {"file": "MEMORY.md", "old": "- Has two cats\n", "new": ""}
        notes.append(said(server_tool("memory", "edit_memory", arguments), "Forgot: ")[:60])
    if joined:
        notes.append("joined " + " + ".join(joined))
    return f"Report {title}: " + "; ".join(notes)


def thread(text):
    """A task's turn on what the user wrote to its thread."""
    words = re.findall(r"^\[Codync: message [^\]]+\]\n([^\n]*)", text, re.M)
    notes = ["picked up" if "in a session that can't be picked up" in text else "same session"]
    if any("NEST" in word for word in words):
        notes.append(said(chat_tool("add_work", {"title": "Inner", "brief": "x"}), "Nested: ")[:60])
    if any("ASK" in word for word in words):
        notes.append(ask())
    hold(" ".join(words))
    return f"Thread {' + '.join(words)}: " + "; ".join(notes)


def answer(text, session):
    """What this turn replies."""
    if "[Codync: your work" in text:
        return relayed(text)
    if "in this work's thread" in text:
        return thread(text)
    if "[Task]" in text:
        return work(text, session)
    if "Scheduled routine:" in text:
        return elsewhere("Routine", text)
    if "sent you this request" in text:
        return elsewhere("Request", text)
    command = re.search(r"^(ADD|STEER|STATUS|MESSAGE)\b ?(.*)$", text, re.M)
    if not command:
        return "reply: " + text.strip().splitlines()[-1]
    name, rest = command.groups()
    if name == "ADD":
        title, brief, *flags = rest.split("|")
        asked = re.search(r"^\[Codync: message ([^\]]+)\]$", text, re.M)
        if asked:
            brief = brief.replace("QUOTE", "QUOTE:" + asked.group(1))
        arguments = {"title": title, "brief": brief}
        for flag in flags:
            if flag.startswith("after="):
                arguments["after"] = item_id(flag.removeprefix("after="))
        added = said(chat_tool("add_work", arguments), "Added: ")
        while "hold" in flags and not cancel.wait(0.01):
            pass
        return added
    if name == "MESSAGE":
        target, request = rest.split("|", 1)
        return said(server_tool("team", "message_bot", {"botId": target, "message": request}), "Messaged: ")[:60]
    if name == "STEER":
        how, title, *more = rest.split("|")
        arguments = {"item": item_id(title), "how": how}
        if more and how == "replace":
            arguments["brief"] = more[0]
        elif more:
            arguments["insteadOf"] = item_id(more[0])
        return said(chat_tool("steer_work", arguments), "Steered: ")
    return said(chat_tool("work_status", {}), "Status: ")


def prompt(request):
    session = request["params"]["sessionId"]
    text = request["params"]["prompt"][0]["text"]
    log("prompts.jsonl", {"session": session, "text": text})
    if pathlib.Path("signedout").exists():
        send({"id": request["id"], "error": SIGNED_OUT})
        return
    reply = answer(text, session)
    if cancel.is_set():
        send({"id": request["id"], "result": {"stopReason": "cancelled"}})
        return
    failure = {"task-signedout": SIGNED_OUT, "task-limit": LIMIT}
    failed = [error for name, error in failure.items() if "[Task]" in text and pathlib.Path(name).exists()]
    asked = pathlib.Path("sign-in-asked")
    if "[Task]" in text and "SIGNIN" in text and not asked.exists():
        asked.write_text("")
        failed.append(SIGNED_OUT)
    if failed:
        send({"id": request["id"], "error": failed[0]})
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
        initialized = {"protocolVersion": 1, "agentCapabilities": {
            "loadSession": CAN_LOAD, "sessionCapabilities": capabilities}}
        if CAN_STEER:
            initialized["_meta"] = {"steering": {"supported": True}}
        send({"id": message["id"], "result": initialized})
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
        joined.clear()
        threading.Thread(target=prompt, args=(message,), daemon=True).start()
    elif method == "_session/steering" and CAN_STEER:
        # The user's words, between Codync's own lines (the message's id before them, a thread's note after).
        words = [line for line in params["prompt"][0]["text"].splitlines() if line and not line.startswith("[Codync:")]
        joined.append(words[-1])
        send({"id": message["id"], "result": {"outcome": "injected"}})
    elif method == "session/cancel":
        cancel.set()
    elif message.get("id") == "permission" and method is None:
        permission.set()
    elif "id" in message and method:
        send({"id": message["id"], "error": {"code": -32601, "message": "not supported"}})
