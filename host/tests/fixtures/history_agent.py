"""ACP fixture for a long chat: every prompt runs tool calls, then answers at once. A prompt naming `<n> tool calls` runs
that many (one turn whose tool rows fill whole history pages), any other runs one; `Message <n>` is answered `Reply <n>`."""
import json
import re
import sys


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def update(value):
    send({"method": "session/update", "params": {"sessionId": "history", "update": value}})


for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "initialize":
        result = {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}
    elif method == "session/new":
        result = {"sessionId": "history"}
    elif method == "session/prompt":
        # The user's words come last, after anything the host puts before them.
        text = " ".join(block.get("text", "") for block in request["params"]["prompt"])
        calls = re.findall(r"(\d+) tool calls", text)
        for i in range(int(calls[-1]) if calls else 1):
            update({"sessionUpdate": "tool_call", "toolCallId": f"{request['id']}-{i}", "title": f"Step {i + 1}",
                    "kind": "read", "status": "completed"})
        number = re.findall(r"Message (\d+)", text)
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"Reply {number[-1] if number else 0}"}})
        result = {"stopReason": "end_turn"}
    else:
        continue
    send({"id": request["id"], "result": result})
