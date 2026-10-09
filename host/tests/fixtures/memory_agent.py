"""Deterministic ACP replies for automatic memory extraction, without an LLM provider."""
import json
import sys
import uuid

session = ""


def send(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    params = request.get("params", {})
    if method == "initialize":
        result = {"agentCapabilities": {"loadSession": True}}
    elif method in ("session/new", "session/load"):
        session = params.get("sessionId", str(uuid.uuid4()))
        result = {"sessionId": session}
    elif method == "session/prompt":
        text = params["prompt"][0]["text"]
        if "Tag each fact you keep with a category" in text:
            reply = "profile: The user prefers concise Traditional Chinese explanations."
        elif "Write a concise session summary" in text:
            reply = "The user established a preference for concise Traditional Chinese explanations."
        else:
            reply = "I will use concise Traditional Chinese explanations."
        send({"method": "session/update", "params": {"sessionId": session, "update": {
            "sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}
        }}})
        result = {"stopReason": "end_turn"}
    else:
        result = {}
    if "id" in request:
        send({"id": request["id"], "result": result})
