"""ACP fixture recording how a bot's instructions and compactions reach it.

`standard` advertises ACP `systemPrompt`; `reporting` reports compactions with `compaction_update`; `silent` only
shows them in `usage_update`; `flaky` resumes sessions and fails its first prompt ever (across restarts), as a rate limit does; `codex` and `claude` answer `initialize` the way those adapters do, so the host asks
them to compact. A prompt containing COMPACT_NOW, or one that is a `/compact` command, compacts; one containing
`USAGE <n>` (optionally `SIZE <m>`) reports that much context in use. Every request is appended to
`requests.jsonl` in the working directory.
"""
import json
import os
import re
import sys

mode = sys.argv[1] if len(sys.argv) > 1 else "reporting"
sessions = 0
compactions = 0


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def update(session, value):
    send({"method": "session/update", "params": {"sessionId": session, "update": value}})


def compact(session):
    global compactions
    compactions += 1
    cid = f"c{compactions}"
    update(session, {"sessionUpdate": "compaction_update", "compactionId": cid, "status": "in_progress"})
    completed = {"sessionUpdate": "compaction_update", "compactionId": cid, "status": "completed"}
    if mode == "claude":
        completed["summary"] = [{"type": "text", "text": f"Summary {compactions}: the user is planning a trip."}]
    update(session, completed)
    update(session, {"sessionUpdate": "usage_update", "used": 20000, "size": 1000000})


for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    params = request.get("params", {})
    with open("requests.jsonl", "a", encoding="utf-8") as log:
        log.write(json.dumps({"method": method, "params": params}) + "\n")
    if method == "initialize":
        capabilities = {"loadSession": mode in ("claude", "flaky"), "sessionCapabilities": {}}
        if mode == "standard":
            capabilities["sessionCapabilities"]["systemPrompt"] = {}
        if mode == "claude":
            capabilities["_meta"] = {"claudeCode": {}}
        result = {"protocolVersion": 1, "agentCapabilities": capabilities}
        if mode == "codex":
            result["agentInfo"] = {"name": "@agentclientprotocol/codex-acp", "version": "0"}
    elif method == "session/new":
        sessions += 1
        result = {"sessionId": f"s{sessions}"}
    elif method == "session/load":
        result = {}
    elif method == "session/prompt":
        session = params["sessionId"]
        text = params["prompt"][0]["text"]
        if mode == "flaky" and not os.path.exists("failed"):
            open("failed", "w").close()
            send({"id": request["id"], "error": {"code": -32000, "message": "rate limited"}})
            continue
        if text.startswith("/compact") or ("COMPACT_NOW" in text and mode != "silent"):
            compact(session)
            if text.startswith("/compact"):
                send({"id": request["id"], "result": {"stopReason": "end_turn"}})
                continue
        usage = re.search(r"USAGE (\d+)", text)
        if usage:
            size = re.search(r"SIZE (\d+)", text)
            update(session, {"sessionUpdate": "usage_update", "used": int(usage.group(1)),
                             "size": int(size.group(1)) if size else 200000})
        update(session, {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "ok"}})
        result = {"stopReason": "end_turn"}
    else:
        result = {}
    if "id" in request:
        send({"id": request["id"], "result": result})
