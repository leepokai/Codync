# A scripted agent that runs subagents as Claude's adapter does (claude-agent-acp 0.109.1): the parent's Agent call, the
# subagent's own calls naming it in _meta.claudeCode.parentToolUseId (another call coming between them), its hand-back
# and totals when a foreground one ends, and a background one that reports none and works on after its launch ends. The turn holds a few seconds, so the working steps can
# be read, then replies. Used by apps/desktop/e2e/subagent-steps.spec.ts and apps/maestro/subagent-steps.yaml.
import json,sys,threading,time
def send(m): sys.stdout.write(json.dumps(m)+"\n"); sys.stdout.flush()
def upd(u): send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":u}})
def agent(extra=None): return {"claudeCode":dict({"toolName":"Agent"},**(extra or {}))}
def child(parent): return {"claudeCode":{"toolName":"Bash","parentToolUseId":parent}}
def turn(rid):
    upd({"sessionUpdate":"tool_call","toolCallId":"a1","title":"Count files","kind":"think","status":"in_progress","_meta":agent(),"rawInput":{"subagent_type":"general-purpose","run_in_background":False}})
    upd({"sessionUpdate":"tool_call","toolCallId":"b1","title":"Print working directory","kind":"think","status":"in_progress","_meta":agent(),"rawInput":{"subagent_type":"Explore","run_in_background":True}})
    upd({"sessionUpdate":"tool_call","toolCallId":"c1","title":"ls -la","kind":"execute","status":"completed","_meta":child("a1")})
    upd({"sessionUpdate":"tool_call","toolCallId":"m1","title":"Read notes.md","kind":"read","status":"completed"})
    upd({"sessionUpdate":"tool_call_update","toolCallId":"b1","_meta":agent({"toolResponse":{"isAsync":True,"status":"async_launched","agentId":"bg"}})})
    upd({"sessionUpdate":"tool_call_update","toolCallId":"b1","status":"completed","_meta":agent()})
    upd({"sessionUpdate":"tool_call","toolCallId":"c2","title":"pwd","kind":"execute","status":"completed","_meta":child("b1")})
    upd({"sessionUpdate":"tool_call","toolCallId":"h1","title":"SubagentHandback","kind":"other","status":"completed","_meta":{"claudeCode":{"toolName":"SubagentHandback","parentToolUseId":"a1"}}})
    upd({"sessionUpdate":"tool_call_update","toolCallId":"a1","_meta":agent({"toolResponse":{"status":"completed","agentType":"general-purpose","totalTokens":42310,"totalToolUseCount":1,"totalDurationMs":6287}})})
    upd({"sessionUpdate":"tool_call_update","toolCallId":"a1","status":"completed","_meta":agent()})
    time.sleep(6)
    upd({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Both subagents are done."}})
    send({"jsonrpc":"2.0","id":rid,"result":{"stopReason":"end_turn"}})
for line in sys.stdin:
    m=json.loads(line)
    meth=m.get("method")
    if meth=="initialize": send({"jsonrpc":"2.0","id":m["id"],"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":False}}})
    elif meth=="session/new": send({"jsonrpc":"2.0","id":m["id"],"result":{"sessionId":"s1"}})
    elif meth=="session/prompt": threading.Thread(target=turn,args=(m["id"],)).start()
