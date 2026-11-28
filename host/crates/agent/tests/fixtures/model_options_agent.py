"""Discovery fixture: two usable models and an advertised but unavailable model."""
import json
import os
import sys
model = "first"
log_path = os.environ.get("ACP_REQUEST_LOG")
for line in sys.stdin:
    msg = json.loads(line)
    if log_path:
        with open(log_path, "a") as f:
            f.write(json.dumps(msg) + "\n")
    if "id" not in msg:
        continue
    requested = msg.get("params", {}).get("value")
    if requested == "unavailable":
        result = {"error": {"code": -32602, "message": "model not available"}}
    else:
        model = requested or model
        level = "high" if model == "second" else "medium"
        result = {"result": {"configOptions": [{"id": "effort", "type": "select", "category": "thought_level",
            "currentValue": level, "options": [{"value": "medium"}, {"value": "high"}]}]}}
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], **result}), flush=True)
