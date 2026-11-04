# Scripted ACP agent: a prompt carrying "run:<command>" makes it ask permission to run that command, then it replies
# with the outcome it got. Used to check which commands an approving bot refuses.
import json, re, sys, threading

def send(m):
    sys.stdout.write(json.dumps(m) + "\n")
    sys.stdout.flush()

pending = {}
next_id = [100]
sid = "s1"

def turn(rid, text):
    found = re.search(r"(run|edit):(.+)", text)
    command = found.group(2).strip() if found else "echo none"
    # "edit:<file>" asks to edit a file; "run:<command>" to run a command.
    kind = "edit" if found and found.group(1) == "edit" else "execute"
    next_id[0] += 1
    pid = next_id[0]
    done = threading.Event()
    pending[pid] = done
    send({"jsonrpc": "2.0", "id": pid, "method": "session/request_permission", "params": {
        "sessionId": sid,
        "toolCall": {"toolCallId": "t1", "title": ("Edit " if kind == "edit" else "Run ") + command, "kind": kind,
                     "rawInput": {} if kind == "edit" else {"command": command}},
        "options": [{"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                    {"optionId": "always", "name": "Always allow", "kind": "allow_always"},
                    {"optionId": "no", "name": "Reject", "kind": "reject_once"}]}})
    done.wait()
    answer = done.answer
    send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": sid, "update": {
        "sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "outcome: " + json.dumps(answer)}}}})
    send({"jsonrpc": "2.0", "id": rid, "result": {"stopReason": "end_turn"}})

for line in sys.stdin:
    m = json.loads(line)
    if "method" not in m:
        done = pending.pop(m["id"], None)
        if done:
            done.answer = m.get("result", {})
            done.set()
        continue
    method = m["method"]
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"sessionId": sid}})
    elif method == "session/prompt":
        text = " ".join(part.get("text", "") for part in m["params"]["prompt"])
        threading.Thread(target=turn, args=(m["id"], text)).start()
