#!/usr/bin/env python3
"""Inert ACP provider for LiveTerminalTest; copy into the isolated fixture host."""
import json
import os
import pathlib
import sys
import uuid

directory = pathlib.Path(os.environ["CODYNC_HOME"]).resolve()
if "codync-android-chat-" not in str(directory) or directory.name != "host":
    raise RuntimeError("This provider requires the isolated Android acceptance host")
marker = directory / "agents/android-setup-fixture/1/signed-in"

if "--terminal" in sys.argv:
    assert sys.stdin.isatty() and sys.stdout.isatty()
    print("\033[32mFixture setup ready: café 🐈\033[0m", flush=True)
    for value in sys.stdin:
        if value.strip() == "fixture-answer":
            marker.write_text("fixture")
            print("Fixture answer accepted", flush=True)
            sys.exit(0)
    sys.exit(1)

for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request:
        continue
    method = request.get("method")
    if method == "initialize":
        result = {"protocolVersion": 1, "agentCapabilities": {}, "authMethods": [
            {"id": "fixture-terminal", "name": "Fixture terminal sign-in", "_meta": {"terminal-auth": {
                "command": sys.executable, "args": [__file__, "--terminal"]}}},
            {"id": "fixture-keys", "name": "Fixture key", "type": "env_var", "vars": [
                {"name": "CODYNC_ACCEPTANCE_KEY", "label": "Fixture key", "secret": True, "optional": False}]},
        ]}
    elif method == "session/new":
        if not marker.exists() and os.environ.get("CODYNC_ACCEPTANCE_KEY") != "fixture-key":
            print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "error": {
                "code": -32000, "message": "Fixture sign-in required"}}), flush=True)
            continue
        result = {"sessionId": str(uuid.uuid4())}
    elif method == "authenticate":
        marker.write_text("fixture")
        result = {}
    else:
        result = {}
    print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}), flush=True)
