"""Codex's API as far as filing a session in a section goes: the handshake, then sections and one move."""

import json
import sys

mode = sys.argv[1]
ready = False
made = False


def answer(message, result=None, error=None):
    body = {"id": message["id"], "result": result} if error is None else {"id": message["id"], "error": error}
    print(json.dumps(body), flush=True)


for line in sys.stdin:
    message = json.loads(line)
    method, params = message["method"], message.get("params", {})
    if method == "initialize":
        answer(message, {})
    elif method == "initialized":
        ready = True
    elif method == "threadSection/list":
        assert ready
        if mode == "old":
            error = {"code": -32600, "message": "Invalid request: unknown variant `threadSection/list`"}
            answer(message, error=error)
        elif mode == "busy":
            answer(message, error={"code": -32603, "message": "database is locked"})
        elif mode == "hang":
            continue
        elif params["cursor"] is None:
            # The section, when there is one, is on the second page.
            answer(message, {"data": [{"id": "pinned", "name": "Pinned"}], "nextCursor": "page-2"})
        else:
            assert params["cursor"] == "page-2"
            found = [] if mode == "missing" else [{"id": "sec-old", "name": "Codync"}]
            answer(message, {"data": found, "nextCursor": None})
    elif method == "threadSection/create":
        assert mode == "missing" and not made and params == {"name": "Codync"}
        made = True
        answer(message, {"section": {"id": "sec-new", "name": "Codync"}})
    elif method == "thread/section/move":
        assert made == (mode == "missing")
        assert params == {"threadId": "thread-1", "sectionId": "sec-new" if made else "sec-old"}
        if mode == "unknown-thread":
            answer(message, error={"code": -32600, "message": "thread not found: thread-1"})
        else:
            answer(message, {})
    else:
        raise AssertionError("Filing a session must not invoke " + method)
