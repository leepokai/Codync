"""ACP fixture returning different model lists for default vs login profiles."""
import json
import os
import sys
import time


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


def should_gate():
    target = os.environ.get("E2E_HOLD_ACCOUNT")
    account = os.environ.get("CODYNC_AGENT_ACCOUNT")
    if target == "*":
        return bool(account)
    if target:
        return account == target
    return True


def maybe_wait_for_gate():
    started = os.environ.get("E2E_PROBE_STARTED")
    release = os.environ.get("E2E_PROBE_RELEASE")
    account = os.environ.get("CODYNC_AGENT_ACCOUNT")
    if not started or not release or not should_gate():
        return
    with open(started, "w", encoding="utf-8") as fh:
        fh.write(account or "default")
    deadline = time.monotonic() + 30.0
    while not os.path.exists(release):
        if time.monotonic() >= deadline:
            raise TimeoutError("timed out waiting for E2E_PROBE_RELEASE")
        time.sleep(0.02)


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
        maybe_wait_for_gate()
        if os.environ.get("E2E_PROBE_FAIL") == "1" and should_gate():
            print(
                json.dumps({"jsonrpc": "2.0", "id": msg["id"], "error": {"code": -32000, "message": "probe failed"}}),
                flush=True,
            )
            continue
        current, models = catalog_for_env()
        result = {
            "sessionId": "s1",
            "models": {"currentModelId": current, "availableModels": models},
        }
    else:
        result = {}
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}), flush=True)
