"""ACP agent that ends a turn with `stopReason: "error"` and no JSON-RPC error behind it, as pi-acp does when a provider
call fails (#470). Any other prompt ends normally, echoing what it was told.
"""

import json
import sys


def send(value):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
    sys.stdout.flush()


session = "erring"
for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method == "initialize":
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        send({"id": message["id"], "result": {"sessionId": session}})
    elif method == "session/prompt":
        text = message["params"]["prompt"][-1]["text"]
        if text.endswith("\nfail"):
            send({"id": message["id"], "result": {"stopReason": "error"}})
            continue
        said = text.rsplit("\n", 1)[-1]
        reply = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"Fine: {said}"}}
        send({"method": "session/update", "params": {"sessionId": session, "update": reply}})
        send({"id": message["id"], "result": {"stopReason": "end_turn"}})
