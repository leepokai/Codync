"""Refuse setup unless the launch argument contains the current credential verbatim."""
import json
import os
import sys

for line in sys.stdin:
    message = json.loads(line)
    if "id" not in message:
        continue
    reply = {"jsonrpc": "2.0", "id": message["id"]}
    if sys.argv[1:] != ["TOKEN=" + os.environ["token"]]:
        reply["error"] = {"code": -32602, "message": "Argument did not match credential"}
    elif message["method"] == "initialize":
        reply["result"] = {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}},
                           "serverInfo": {"name": "argument-fixture", "version": "1"}}
    else:
        reply["result"] = {"tools": []}
    print(json.dumps(reply), flush=True)
