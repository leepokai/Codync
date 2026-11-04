# Scripted ACP agent: the first prompt ends at the step limit (max_turn_requests), a prompt that asks it to continue
# ends normally. Used to check that a half-done turn is not reported as finished.
import json, sys

def send(m):
    sys.stdout.write(json.dumps(m) + "\n")
    sys.stdout.flush()

sid = "s1"

def say(text):
    send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": sid, "update": {
        "sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}}})

for line in sys.stdin:
    m = json.loads(line)
    if "method" not in m:
        continue
    method = m["method"]
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"sessionId": sid}})
    elif method == "session/prompt":
        text = " ".join(part.get("text", "") for part in m["params"]["prompt"])
        if "Continue where you left off" in text:
            say("All done now.")
            send({"jsonrpc": "2.0", "id": m["id"], "result": {"stopReason": "end_turn"}})
        else:
            say("Halfway through the work.")
            send({"jsonrpc": "2.0", "id": m["id"], "result": {"stopReason": "max_turn_requests"}})
