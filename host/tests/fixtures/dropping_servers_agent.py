"""ACP agent that says it is OpenCode 2.x and runs the MCP servers a session is opened with, as OpenCode does. Prompts
steer it: `EVICT` ends those servers and keeps the session, as OpenCode's idle eviction does (#508); `DETACH` starts a
process in a session of its own, like a dev server, and `ATTACH` ends it. It logs every request, with its pid, to
requests.jsonl in its folder."""

import json
import os
import subprocess
import sys
import uuid

servers, detached = [], []


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def run_servers(params):
    for server in params.get("mcpServers", []):
        env = {**os.environ, **{e["name"]: e["value"] for e in server.get("env", [])}}
        command = [server["command"], *server.get("args", [])]
        servers.append(subprocess.Popen(command, env=env, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL))


def end(processes):
    for process in processes:
        process.kill()
        process.wait()
    processes.clear()


for line in sys.stdin:
    message = json.loads(line)
    method, rid = message.get("method"), message.get("id")
    if method is None or rid is None:
        continue
    params = message.get("params", {})
    with open("requests.jsonl", "a", encoding="utf-8") as log:
        log.write(json.dumps({"method": method, "params": params, "pid": os.getpid()}) + "\n")
    if method == "initialize":
        result = {"protocolVersion": 1, "agentCapabilities": {"loadSession": True},
                  "agentInfo": {"name": "OpenCode", "version": "2.3.2"}}
        send({"id": rid, "result": result})
    elif method == "session/new":
        run_servers(params)
        send({"id": rid, "result": {"sessionId": str(uuid.uuid4())}})
    elif method == "session/load":
        run_servers(params)
        send({"id": rid, "result": {}})
    elif method == "session/prompt":
        text = params["prompt"][0]["text"]
        if "EVICT" in text:
            end(servers)
        if "DETACH" in text:
            sleeper = [sys.executable, "-c", "import time; time.sleep(120)"]
            detached.append(subprocess.Popen(sleeper, stdin=subprocess.DEVNULL, start_new_session=True))
        if "ATTACH" in text:
            end(detached)
        update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "ok"}}
        send({"method": "session/update", "params": {"sessionId": params["sessionId"], "update": update}})
        send({"id": rid, "result": {"stopReason": "end_turn"}})
    else:
        send({"id": rid, "error": {"code": -32601, "message": f"Method not found: {method}"}})
