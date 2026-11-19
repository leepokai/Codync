"""ACP agent that can't reopen a session (no `loadSession`), for the connector edit test. Each new session starts the
remote connector's proxy the host lists in its `mcpServers` and keeps it for the session, as agents do; every prompt
calls the connector's `whoami` tool through it and answers "connector said <text>" (or "connector failed: <error>").
Every request goes to `requests.jsonl` in its folder."""

import json
import os
import subprocess
import sys

proxy = None
ids = iter(range(1, 1_000_000))


def send(value):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
    sys.stdout.flush()


def mcp(method, params=None):
    """One MCP request to the proxy: its answer, after any other line it writes."""
    request_id = next(ids)
    proxy.stdin.write(json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params or {}}) + "\n")
    proxy.stdin.flush()
    for line in proxy.stdout:
        reply = json.loads(line)
        if reply.get("id") == request_id:
            return reply
    sys.exit("the connector's proxy quit")


def start_proxy(servers):
    global proxy
    server = next(s for s in servers if "remote" in s["args"])
    env = {**os.environ, **{e["name"]: e["value"] for e in server.get("env", [])}}
    proxy = subprocess.Popen(
        [server["command"], *server["args"]], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True
    )
    mcp("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "fixture"}})
    proxy.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
    proxy.stdin.flush()


for line in sys.stdin:
    message = json.loads(line)
    method, params = message.get("method"), message.get("params") or {}
    if "id" not in message or method is None:
        continue
    with open("requests.jsonl", "a", encoding="utf-8") as log:
        log.write(json.dumps({"method": method, "params": params}) + "\n")
    if method == "initialize":
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        start_proxy(params.get("mcpServers", []))
        send({"id": message["id"], "result": {"sessionId": f"s{os.getpid()}"}})
    elif method == "session/prompt":
        answer = mcp("tools/call", {"name": "whoami", "arguments": {}})
        if "error" in answer:
            text = "connector failed: " + answer["error"]["message"]
        else:
            text = "connector said " + answer["result"]["content"][0]["text"]
        chunk = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}
        send({"method": "session/update", "params": {"sessionId": params["sessionId"], "update": chunk}})
        send({"id": message["id"], "result": {"stopReason": "end_turn"}})
    else:
        send({"id": message["id"], "error": {"code": -32601, "message": "not supported"}})
