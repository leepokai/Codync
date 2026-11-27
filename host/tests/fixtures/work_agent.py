"""Deterministic ACP agent for the tests of a bot's box: a bot that adds and steers work, and the thread that does it.

In the chat: `ADD title|brief[|hold][|after=<title>][|folder=<path>][|place=<here or worktree>]`, `STEER how|title[|brief, or the title that gives way]`,
`TELL title|words[|now]` (passes words on to a task), `ASKTASK title|question` (asks a working task), `STATUS`, `MESSAGE <bot id>|<request>` (asks another bot),
`RECALL words` (looks back at the first task `recall_task` finds with them: its status, result and thread's steps). In a thread, the brief says what it does: `HOLD:<name>`
works until a `release-<name>` file is in its folder, `PERMISSION` asks for an approval (`PERMISSION:<name>`: once
`release-<name>` is there), `ASK` asks the user to
choose, `SHARE` sends a file (`QUOTE`: quoting the chat message that added the task), `ARTIFACT` publishes one,
`NEST` tries what a thread may not, `CHATQ:<question>;` asks its chat (`ask_chat`, its result logged to `ask_chat.jsonl`), `REMEMBER` adds a line to `MEMORY.md` with `edit_memory` and `FORGET` tries to take
one out (both once it is released), `TOUCH` writes `made.txt` in its folder, `DETACH:<name>` starts a command in a
session of its own (its pid in `detached.pid`) that writes `detached-done` once `release-<name>` is there, and ends its
turn while that command still runs: asked by the host to wait for what it left running, it waits for `detached-done`
and reports, or ends at once again with `AGAIN` in its brief. `WORKFLOW:<name>` (to a host that asked for
`asyncTasks` in `initialize`, which `initialize.jsonl` logs, of an agent started with `tasks`) starts a workflow the way Claude's adapter reports one and
ends its turn; once `release-<name>` is there the workflow ends and the agent begins a turn by itself that reports
(`STOP` in the brief: the workflow is stopped instead, and no such turn follows). `LOOP` makes the same tool call six
times, `COMMAND` starts a shell command with a long title (`LONG_COMMAND`), which the task shows as what it does.
A thread's question (`ask_chat`) the chat answers with `answer_task` (`Chat: <the question>`) and `(pass)`;
`QUIET` in it: it doesn't answer; `DECIDE`: it tries `request_decision` first (logged to `chat_decision.jsonl`)
and answers that only the user can decide.
The watchdog's turn in the chat answers `Watchdog: <why the task looks stuck>`, or `(pass)` for a task whose title
has `PASS`.

A routine run and another bot's request read the same words in the routine's instruction or the request, and also
`SEND` (a message to the user), `PROGRESS` (saves the routine's progress note), `FOLLOW` (sets a follow-up) and
`PASS` (nothing to tell).

What the chat passes on to a working task's thread is a turn of its own unless it joins the running one: it answers
`Heard <the words>`.

What the user writes to a thread is a turn of its own: it answers `Thread <their words>: <notes>`, where the
notes say whether the session is the one that did the work (`same session`) or a new one told of it (`picked up`),
and the words may hold `HOLD:<name>`, `NEST` and `ASK` as a brief does.

Arguments: `as=<name>`: a task's report says `on <name>` (which agent did it). `no-load`: the agent doesn't advertise `session/load`, so a
session can't be picked up again. It always advertises `session/fork`, which no thread may take. `steer`: it takes a message into its running turn (`_session/steering`), and the turn's reply
ends with `joined <the message>`; the bot's question (`ask_task`) it answers at once with `Answer: <the question>` (`SLOW` in it: after 62 seconds), and
a late answer's turn in the chat replies `Late: <the answer>`. A `no-start` file in its folder: it doesn't start.
A `signedout` file: every prompt fails asking to sign in (ACP's -32000). `thread-signedout` / `thread-limit`: a thread's
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
import time

sys.stdin.reconfigure(encoding="utf-8")
sys.stdout.reconfigure(encoding="utf-8")

if pathlib.Path("no-start").exists():
    sys.exit(1)

CAN_LOAD = "no-load" not in sys.argv
CAN_STEER = "steer" in sys.argv
# Like Claude's adapter, it reports its background work to a host that asks (`asyncTasks`).
HAS_TASKS = "tasks" in sys.argv
AGENT = next((arg.removeprefix("as=") for arg in sys.argv if arg.startswith("as=")), None)
# What the user's messages steered into the running turn said.
joined = []
SIGNED_OUT = {"code": -32000, "message": "Authentication required"}
LIMIT = {
    "code": -32603,
    "message": "Internal error: You've hit your limit · resets 3pm (Asia/Taipei)",
    "data": {"errorKind": "rate_limit"},
}
LONG_COMMAND = (
    "agentctl wait 3f2a9c1e-7b44-4d1a-9e0f-0a1b2c3d4e5f --until idle --timeout 3600"
    " --workspace /home/example/.codync/bots/0bcd2c28-ac38-4db8-b932-4ec2da5be7a3/workspace"
)
# What the thread's task left running in the background (`DETACH`), for the host's ask to wait for it.
left = {}
output_lock = threading.Lock()
cancel = threading.Event()
permission = threading.Event()
servers = []
forked = set()
# The host asked for the work it runs in the background (`asyncTasks`).
reports_tasks = False
count = 0
# Prompts this process took so far, which `closed.jsonl` says of each session it closed.
prompted = 0


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
    server = next((s for s in servers if s["name"] == name), None)
    if server is None:
        # A thread isn't given that server at all.
        return {"isError": True, "content": [{"type": "text", "text": f"no {name} server"}]}
    with subprocess.Popen([server["command"], *server["args"]], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, encoding="utf-8") as mcp:
        def rpc(identifier, method, params):
            mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}) + "\n")
            mcp.stdin.flush()
            return json.loads(mcp.stdout.readline())

        rpc(1, "initialize", {"protocolVersion": "2025-06-18"})
        listed = {t["name"] for t in rpc(2, "tools/list", {})["result"]["tools"]}
        # A thread's chat server leaves out what only the chat may call; the host still refuses a call to it.
        log("listed.jsonl", {"server": name, "tool": tool, "listed": tool in listed})
        response = rpc(3, "tools/call", {"name": tool, "arguments": arguments})
        mcp.stdin.close()
        return response["result"]


def said(response, done):
    return ("Refused: " if response.get("isError") else done) + response["content"][0]["text"]


def task_id(title):
    """The id of the bot's task called `title`."""
    work = json.loads(chat_tool("work_status", {})["content"][0]["text"])["tasks"]
    return next(w["taskId"] for w in work if w["title"] == title)


def relayed(text):
    """What the chat says of the reports in `text`: each task's report, or why it didn't finish."""
    reports = text.split("\n\n[Codync: these are reports", 1)[0].removeprefix("[Codync: your task ")
    parts = [part.partition("]") for part in reports.split("\n\n[Codync: your task ")]
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
        notes.append(said(chat_tool("add_task", {"title": "Inner", "brief": "x"}), "Nested: ")[:60])
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
    """A thread's turn on its task."""
    lead, rest = text.split("[Task]", 1)[1].split("\n\n", 1)
    title = lead.split('"')[1]
    brief = rest.split("\n\n[", 1)[0]
    notes = ["next" if lead.startswith(" Next") else "forked" if session in forked else "briefed",
             "saw the chat" if "[The chat you come from, so far]" in text else "no chat"]
    if "set aside" in rest:
        notes.append("again")
    asked = re.search(r"CHATQ:([^;]+);", brief)
    if asked:
        result = chat_tool("ask_chat", {"question": asked.group(1)})
        log("ask_chat.jsonl", result)
        notes.append(said(result, "Chat said: ")[:120])
    if "ASK" in brief:
        notes.append(ask())
    if "SHARE" in brief:
        pathlib.Path("shared.txt").write_text("from a thread", encoding="utf-8")
        quoted = re.search(r"QUOTE:(\S+)", brief)
        arguments = {"path": "shared.txt", **({"reply_to": quoted.group(1)} if quoted else {})}
        notes.append(said(chat_tool("send_file", arguments), "Shared: ")[:60])
    if "ARTIFACT" in brief:
        pathlib.Path("page.html").write_text("<h1>Page</h1>", encoding="utf-8")
        notes.append(said(chat_tool("publish_artifact", {"path": "page.html", "title": "Page"}), "Published: ")[:40])
    if "NEST" in brief:
        notes.append(said(chat_tool("add_task", {"title": "Inner", "brief": "x"}), "Nested: ")[:60])
    if "TOUCH" in brief:
        pathlib.Path("made.txt").write_text("made by a thread", encoding="utf-8")
    detach = re.search(r"DETACH:(\w+)", brief)
    if detach:
        # Like a review or `gh run watch` the agent left running in the background; gives up after a minute.
        wait = f'i=0; while [ ! -e release-{detach.group(1)} ] && [ $i -lt 600 ]; do sleep 0.1; i=$((i+1)); done'
        started = subprocess.Popen(["/bin/sh", "-c", f"{wait}; echo done > detached-done"], start_new_session=True,
                                   stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        pathlib.Path("detached.pid").write_text(str(started.pid), encoding="utf-8")
        notes.append("started the review, waiting for it to finish")
        left.update(title=title, again="AGAIN" in brief, detached=True)
    workflow = re.search(r"WORKFLOW:(\w+)", brief)
    if workflow and reports_tasks:
        for update in (
            {"sessionUpdate": "tool_call", "toolCallId": "workflow", "title": "Workflow", "kind": "other",
             "status": "completed", "_meta": {"claudeCode": {"toolName": "Workflow"}}},
            {"sessionUpdate": "async_task_spawned", "asyncTaskId": "w1", "name": "after-change", "taskType": "workflow",
             "description": "after-change", "toolCallId": "workflow"},
            {"sessionUpdate": "async_task_progress", "asyncTaskId": "w1", "description": "Review: diff"},
        ):
            send({"method": "session/update", "params": {"sessionId": session, "update": update}})
        threading.Thread(target=run_workflow, args=(session, workflow.group(1), title, "STOP" in brief),
                         daemon=True).start()
        notes.append("started the workflow, waiting for it to finish")
    if "LOOP" in brief:
        for n in range(6):
            send({"method": "session/update", "params": {"sessionId": session, "update": {
                "sessionUpdate": "tool_call", "toolCallId": f"loop-{n}", "title": "Read the log", "status": "completed"}}})
    if "COMMAND" in brief:
        send({"method": "session/update", "params": {"sessionId": session, "update": {
            "sessionUpdate": "tool_call", "toolCallId": "command", "title": LONG_COMMAND, "kind": "execute",
            "status": "in_progress",
        }}})
    if "PERMISSION" in brief:
        later = re.search(r"PERMISSION:(\w+)", brief)
        if later:
            hold("HOLD:" + later.group(1))
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
    if AGENT:
        notes.append("on " + AGENT)
    return f"Report {title}: " + "; ".join(notes)


def run_workflow(session, name, title, stopped):
    """The workflow `WORKFLOW:<name>` started: it ends once `release-<name>` is there, and the agent's own turn that
    answers its end reports (none after a stop)."""
    while not pathlib.Path("release-" + name).exists():
        time.sleep(0.05)
    updates = [{"sessionUpdate": "async_task_state_update", "asyncTaskId": "w1",
                "state": "stopped" if stopped else "completed"}]
    if not stopped:
        updates += [
            {"sessionUpdate": "async_task_spawned", "asyncTaskId": "turn-1", "name": "Claude is working",
             "taskType": "turn", "description": 'Dynamic workflow "after-change" completed'},
            {"sessionUpdate": "tool_call", "toolCallId": "merge", "title": "git merge", "kind": "execute",
             "status": "completed"},
            {"sessionUpdate": "agent_message_chunk",
             "content": {"type": "text", "text": f"Report {title}: the workflow passed"}},
            {"sessionUpdate": "async_task_state_update", "asyncTaskId": "turn-1", "state": "completed"},
        ]
    for update in updates:
        send({"method": "session/update", "params": {"sessionId": session, "update": update}})


def thread(text):
    """A thread's turn on what the user wrote to its thread."""
    words = re.findall(r"^\[Codync: message [^\]]+\]\n([^\n]*)", text, re.M)
    notes = ["picked up" if "in a session that can't be picked up" in text else "same session"]
    if any("NEST" in word for word in words):
        notes.append(said(chat_tool("add_task", {"title": "Inner", "brief": "x"}), "Nested: ")[:60])
    if any("ASK" in word for word in words):
        notes.append(ask())
    hold(" ".join(words))
    return f"Thread {' + '.join(words)}: " + "; ".join(notes)


ASKED = "[Codync: the bot that gave you this task asks you this"


def answer_asked(session, question):
    """The bot's question, steered into a running turn: one short message, closed by a step, then on with the task.
    `SLOW` in it answers after the bot's call stopped waiting."""
    if "SLOW" in question:
        cancel.wait(62)
    for update in ({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Answer: " + question}},
                   {"sessionUpdate": "tool_call", "toolCallId": "after-answer", "title": "Go on", "status": "completed"}):
        send({"method": "session/update", "params": {"sessionId": session, "update": update}})


def chat_answer(text):
    """The chat's turn on its thread's question (`ask_chat`): answers it with `answer_task`, then passes."""
    task, question = re.search(r"\(([^)]+)\) asks you this while it works[^\n]*\n([^\n]*)", text).groups()
    if "QUIET" in question:
        return "(pass)"
    answer = "Chat: " + question
    if "DECIDE" in question:
        log("chat_decision.jsonl", chat_tool("request_decision", {"question": "Deploy?", "options": ["Yes", "No"]}))
        answer = "Only the user can decide that: ask them"
    log("answer_task.jsonl", chat_tool("answer_task", {"task": task, "answer": answer}))
    return "(pass)"


def waited():
    """The host asked it to wait for what it left running: it does and reports, or (`AGAIN`) ends at once again."""
    if left.get("again"):
        return "Still waiting for it"
    while left.get("detached") and not pathlib.Path("detached-done").exists() and not cancel.wait(0.05):
        pass
    return f"Report {left.get('title')}: it finished, all green"


def answer(text, session):
    """What this turn replies."""
    if "[Codync: you ended your turn while" in text:
        return waited()
    if "[Codync: your background work" in text:
        return "Report after the stop"
    if ASKED in text:
        # An agent that can't take it mid-turn: the question is a turn of its own.
        return "Answer: " + text.split("\n\n" + ASKED, 1)[0].strip().splitlines()[-1]
    if "answered the question you asked it" in text:
        return "Late: " + re.search(r"after your call stopped waiting\.\]\n([^\n]*)", text).group(1)
    if "[Codync watchdog]" in text:
        return "(pass)" if "PASS" in text else "Watchdog: " + text.split("looks stuck: ", 1)[1].split(". Look at it", 1)[0]
    if "asks you this while it works" in text:
        return chat_answer(text)
    if "[Codync: your task" in text:
        return relayed(text)
    if "[Codync: your chat passed this on" in text:
        return "Heard " + text.split("\n\n[Codync: your chat passed this on", 1)[0].strip().splitlines()[-1]
    if "in this task's thread" in text:
        return thread(text)
    if "[Task]" in text:
        return work(text, session)
    if "Scheduled routine:" in text:
        return elsewhere("Routine", text)
    if "sent you this request" in text:
        return elsewhere("Request", text)
    command = re.search(r"^(ADD|STEER|TELL|ASKTASK|STATUS|MESSAGE|RECALL)\b ?(.*)$", text, re.M)
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
                arguments["after"] = task_id(flag.removeprefix("after="))
            if flag.startswith("folder="):
                arguments["folder"] = flag.removeprefix("folder=")
            if flag.startswith("place="):
                arguments["place"] = flag.removeprefix("place=")
        added = said(chat_tool("add_task", arguments), "Added: ")
        while "hold" in flags and not cancel.wait(0.01):
            pass
        return added
    if name == "MESSAGE":
        target, request = rest.split("|", 1)
        return said(server_tool("team", "message_bot", {"botId": target, "message": request}), "Messaged: ")[:60]
    if name == "STEER":
        how, title, *more = rest.split("|")
        arguments = {"task": task_id(title), "how": how}
        if more and how == "replace":
            arguments["brief"] = more[0]
        elif more:
            arguments["insteadOf"] = task_id(more[0])
        return said(chat_tool("steer_task", arguments), "Steered: ")
    if name == "TELL":
        title, words, *flags = rest.split("|")
        arguments = {"task": task_id(title), "text": words, "now": "now" in flags}
        return said(chat_tool("tell_task", arguments), "Told: ")
    if name == "ASKTASK":
        title, question = rest.split("|", 1)
        return said(chat_tool("ask_task", {"task": task_id(title), "question": question}), "Asked: ")
    if name == "RECALL":
        found = json.loads(chat_tool("recall_task", {"query": rest})["content"][0]["text"])["tasks"]
        if not found:
            return "Recalled nothing"
        task = json.loads(chat_tool("recall_task", {"task": found[0]["taskId"]})["content"][0]["text"])
        return f"Recalled {task['title']}: {task['status']} | {task['result']} | {len(task['steps'])} steps"
    return said(chat_tool("work_status", {}), "Status: ")


def prompt(request):
    session = request["params"]["sessionId"]
    text = request["params"]["prompt"][0]["text"]
    log("prompts.jsonl", {"session": session, "text": text, "pid": os.getpid()})
    if pathlib.Path("signedout").exists():
        send({"id": request["id"], "error": SIGNED_OUT})
        return
    reply = answer(text, session)
    if cancel.is_set():
        send({"id": request["id"], "result": {"stopReason": "cancelled"}})
        return
    failure = {"thread-signedout": SIGNED_OUT, "thread-limit": LIMIT}
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
        asked = params.get("clientCapabilities", {}).get("_meta", {}).get("jetbrains", {}).get("air", {})
        reports_tasks = HAS_TASKS and "asyncTasks" in asked.get("capabilities", [])
        log("initialize.jsonl", {"asyncTasks": "asyncTasks" in asked.get("capabilities", [])})
        initialized = {"protocolVersion": 1, "agentCapabilities": {
            "loadSession": CAN_LOAD, "sessionCapabilities": {"close": {}, "fork": {}}}, "_meta": {}}
        if CAN_STEER:
            initialized["_meta"]["steering"] = {"supported": True}
        if reports_tasks:
            initialized["_meta"]["jetbrains"] = {"air": {"version": 1, "capabilities": ["asyncTasks"]}}
        send({"id": message["id"], "result": initialized})
    elif method in ("session/new", "session/fork", "session/load"):
        servers = params.get("mcpServers", servers)
        session = params["sessionId"] if method == "session/load" else new_session()
        if method == "session/fork":
            forked.add(session)
            log("forks.jsonl", {"from": params["sessionId"], "to": session})
        if method != "session/load":
            log("servers.jsonl", {"session": session, "servers": [s["name"] for s in servers]})
        send({"id": message["id"], "result": {} if method == "session/load" else {"sessionId": session}})
    elif method == "session/close":
        log("closed.jsonl", {"session": params.get("sessionId"), "prompts": prompted})
        send({"id": message["id"], "result": {}})
    elif method == "session/prompt":
        prompted += 1
        cancel.clear()
        permission.clear()
        joined.clear()
        threading.Thread(target=prompt, args=(message,), daemon=True).start()
    elif method == "_session/steering" and CAN_STEER:
        # The user's words, between Codync's own lines (the message's id before them, a thread's note after).
        text = params["prompt"][0]["text"]
        words = [line for line in text.splitlines() if line and not line.startswith("[Codync:")]
        if ASKED in text:
            threading.Thread(target=answer_asked, args=(params["sessionId"], words[-1]), daemon=True).start()
        else:
            joined.append(words[-1])
        send({"id": message["id"], "result": {"outcome": "injected"}})
    elif method == "session/cancel":
        cancel.set()
    elif message.get("id") == "permission" and method is None:
        permission.set()
    elif "id" in message and method:
        send({"id": message["id"], "error": {"code": -32601, "message": "not supported"}})
