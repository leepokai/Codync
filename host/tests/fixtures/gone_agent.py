"""ACP agent that outlives its own connection: told "gone" it says it is working, closes its stdout and lingers, so the
host is left with a turn open and no agent to answer it (#483). Any other prompt ends normally, echoing what it was told.
"""

import json
import os
import sys
import time


def send(value):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
    sys.stdout.flush()


session = "gone"
for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method == "initialize":
        send({"id": message["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        send({"id": message["id"], "result": {"sessionId": session}})
    elif method == "session/prompt":
        text = message["params"]["prompt"][-1]["text"]
        if text.endswith("\ngone"):
            # The connection goes while the turn is open: the host has nothing left to ask or to kill.
            working = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Working on it"}}
            send({"method": "session/update", "params": {"sessionId": session, "update": working}})
            os.close(1)
            time.sleep(30)
            break
        said = text.rsplit("\n", 1)[-1]
        reply = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": f"Back again: {said}"}}
        send({"method": "session/update", "params": {"sessionId": session, "update": reply}})
        send({"id": message["id"], "result": {"stopReason": "end_turn"}})
