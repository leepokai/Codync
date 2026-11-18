"""ACP agent with its own `/` commands, like Claude Code's adapter: right after `session/new` it sends an
`available_commands_update` (review, with an input hint, and compact, or the list in commands.json in its folder);
reopening a session (`session/load`), it sends the list among the replay, before it answers. It answers each prompt
with `ok` and logs every request to requests.jsonl in its folder."""

import json
import os
import sys

COMMANDS = [
    {"name": "review", "description": "Review the changes", "input": {"hint": "what to look at"}},
    {"name": "compact", "description": "Summarize the conversation"},
]


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def update(content):
    send({"method": "session/update", "params": {"sessionId": "s1", "update": content}})


for line in sys.stdin:
    message = json.loads(line)
    method, rid = message.get("method"), message.get("id")
    if method is None or rid is None:
        continue
    params = message.get("params", {})
    with open("requests.jsonl", "a", encoding="utf-8") as log:
        log.write(json.dumps({"method": method, "params": params}) + "\n")
    if method == "initialize":
        send({"id": rid, "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": True}}})
    elif method in ("session/new", "session/load"):
        commands = COMMANDS
        if os.path.exists("commands.json"):
            with open("commands.json", encoding="utf-8") as f:
                commands = json.load(f)
        listed = {"sessionUpdate": "available_commands_update", "availableCommands": commands}
        if method == "session/load":
            update(listed)
        send({"id": rid, "result": {"sessionId": "s1"}})
        if method == "session/new":
            update(listed)
    elif method == "session/prompt":
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "ok"}})
        send({"id": rid, "result": {"stopReason": "end_turn"}})
    else:
        send({"id": rid, "result": {}})
