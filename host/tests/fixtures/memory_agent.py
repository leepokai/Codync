"""Deterministic ACP replies for dreaming, without an LLM provider.

A dream run gets a plan with two additions: one quoting what the user typed, one quoting what they pasted. Only
the first may survive the host's audit. It also appends a note to a long-form file.

A run told to tidy a nearly full MEMORY.md instead moves each line starting "Allergic" into a note named after this
process, summed up as "Has food allergies", so two runs working from the same copy would leave the line in two
notes. Every dream run is logged to `dreams.log` in the working directory: `tidy` or `dream`.
"""
import json
import os
import sys
import uuid

session = ""

PLAN = {
    "add": [
        {
            "text": "Prefers concise Traditional Chinese explanations",
            "quote": "explain things to me concisely in Traditional Chinese",
            "message": "m1",
        },
        {"text": "Loves durian", "quote": "I love durian", "message": "m1"},
    ],
    "archive": [],
    "review": [],
    "append": [
        {
            "file": "style.md",
            "text": "Wants things explained concisely, in Traditional Chinese",
            "quote": "explain things to me concisely",
            "message": "m1",
            "description": "How the user likes answers",
        },
    ],
}


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
        if "You keep the memory a personal assistant bot has" in text:
            tidy = '"moveOut"' in text
            with open("dreams.log", "a", encoding="utf-8") as log:
                log.write("tidy\n" if tidy else "dream\n")
            plan = PLAN
            if tidy:
                note = f"health/allergies-{os.getpid()}.md"
                moving = [line[2:] for line in text.splitlines() if line.startswith("- Allergic")]
                plan = {"moveOut": [{"line": line, "shared": True, "file": note, "description": "The user's allergies",
                                     "summary": "Has food allergies"} for line in moving]}
            reply = "Here is the plan:\n" + json.dumps(plan)
        elif "You name a personal assistant bot" in text:
            reply = "NONE"
        else:
            reply = "I will keep my explanations short."
        send({"method": "session/update", "params": {"sessionId": session, "update": {
            "sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}
        }}})
        result = {"stopReason": "end_turn"}
    else:
        result = {}
    if "id" in request:
        send({"id": request["id"], "result": result})
