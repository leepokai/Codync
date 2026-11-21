"""ACP fixture returning different model lists for default vs login profiles."""
import json
import os
import sys


def catalog_for_env():
    tier = os.environ.get("E2E_MODEL_TIER")
    account = os.environ.get("CODYNC_AGENT_ACCOUNT")
    if tier == "ultra":
        return "ultra-carbon", [
            {"modelId": "ultra-neon", "name": "Ultra Neon"},
            {"modelId": "ultra-carbon", "name": "Ultra Carbon"},
        ]
    if account:
        return f"acct-{account}-pro", [
            {"modelId": f"acct-{account}-pro", "name": "Account Pro"},
            {"modelId": "shared-flash", "name": "Shared Flash"},
        ]
    return "default-flash", [
        {"modelId": "default-flash", "name": "Default Flash"},
    ]


for raw in sys.stdin:
    line = raw.strip()
    if not line:
        continue
    msg = json.loads(line)
    if "id" not in msg:
        continue
    method = msg.get("method")
    if method == "initialize":
        result = {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}
    elif method == "session/new":
        current, models = catalog_for_env()
        result = {
            "sessionId": "s1",
            "models": {"currentModelId": current, "availableModels": models},
        }
    else:
        result = {}
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
