# A minimal ACP agent for the usage readings: logs each method it's asked, advertises the slash
# command named by USAGE_AGENT_COMMAND once a session opens, answers a prompt with a report, and
# answers Grok's billing extension.
import json, os, sys

log = os.environ["USAGE_AGENT_LOG"]
command = os.environ["USAGE_AGENT_COMMAND"]


def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def update(session, body):
    send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": session, "update": body}})


def answer(request, result):
    send({"jsonrpc": "2.0", "id": request["id"], "result": result})


def advertise(session):
    update(session, {"sessionUpdate": "available_commands_update", "availableCommands": [{"name": command}]})


for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method is None:
        continue
    with open(log, "a") as calls:
        calls.write(method + "\n")
    if method == "initialize":
        answer(request, {"protocolVersion": 1, "agentCapabilities": {"sessionCapabilities": {"resume": {}}}})
    elif method == "x.ai/billing":
        period = {"type": "USAGE_PERIOD_TYPE_WEEKLY"}
        answer(request, {"config": {"creditUsagePercent": 42.5, "currentPeriod": period}})
    elif method == "session/new":
        answer(request, {"sessionId": "s1"})
        advertise("s1")
    elif method == "session/resume":
        # A resumed session replays its history, which isn't the command's reply.
        update("s1", {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "old "}})
        answer(request, {})
        advertise("s1")
    elif method == "session/prompt":
        update("s1", {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "the report"}})
        answer(request, {"stopReason": "end_turn"})
