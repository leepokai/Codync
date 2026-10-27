"""Only the read-only Codex account handshake is accepted; no thread or turn may start."""

import json
import sys

mode = sys.argv[1]
initialized = False
ready = False

for line in sys.stdin:
    message = json.loads(line)
    method = message["method"]
    if method == "initialize":
        assert not initialized
        assert message["params"]["clientInfo"]["name"] == "codync"
        if mode == "bad-handshake":
            print(json.dumps({"id": message["id"], "error": {"code": -32600, "message": "initialize rejected"}}), flush=True)
            continue
        initialized = True
        result = {}
    elif method == "initialized":
        assert initialized
        ready = True
        continue
    elif method == "account/rateLimits/read":
        assert ready
        if mode == "hang":
            continue
        if mode == "signed-out":
            print(json.dumps({"id": message["id"], "error": {"code": -32000, "message": "Sign in required"}}), flush=True)
            continue
        result = {"rateLimits": {"primary": {"usedPercent": 23, "windowDurationMins": 10080, "resetsAt": 2000000000}}}
    else:
        raise AssertionError("A usage probe must not invoke " + method)
    print(json.dumps({"id": message["id"], "result": result}), flush=True)
