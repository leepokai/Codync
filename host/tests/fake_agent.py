# Minimal ACP agent for the end-to-end test: one text chunk, one tool call that
# needs permission, then a final reply echoing the permission outcome.
import json,sys,threading,time
def send(m): sys.stdout.write(json.dumps(m)+"\n"); sys.stdout.flush()
pending={}
nid=[100]
sid="s1"
def upd(u): send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":sid,"update":u}})
def turn(rid):
    upd({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Checking…"}})
    upd({"sessionUpdate":"tool_call","toolCallId":"t1","title":"Edit a.txt","kind":"edit","status":"pending","content":[{"type":"diff","path":"a.txt","oldText":"x\n","newText":"y\n"}]})
    nid[0]+=1; pid=nid[0]
    ev=threading.Event(); pending[pid]=ev
    send({"jsonrpc":"2.0","id":pid,"method":"session/request_permission","params":{"sessionId":sid,"toolCall":{"toolCallId":"t1","title":"Edit a.txt","kind":"edit","rawInput":{"command":"sed -i s/x/y/ a.txt"}},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},{"optionId":"always","name":"Always","kind":"allow_always"},{"optionId":"no","name":"Reject","kind":"reject_once"}]}})
    ev.wait()
    ans=ev.answer
    if ans.get("outcome",{}).get("outcome")=="cancelled":
        send({"jsonrpc":"2.0","id":rid,"result":{"stopReason":"cancelled"}}); return
    upd({"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"completed"})
    upd({"sessionUpdate":"plan","entries":[{"content":"edit","status":"completed","priority":"high"}]})
    upd({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Done: "+json.dumps(ans)}})
    send({"jsonrpc":"2.0","id":rid,"result":{"stopReason":"end_turn"}})
for line in sys.stdin:
    m=json.loads(line)
    if "method" not in m:
        ev=pending.pop(m["id"],None)
        if ev: ev.answer=m.get("result",{}); ev.set()
        continue
    meth=m["method"]
    if meth=="initialize": send({"jsonrpc":"2.0","id":m["id"],"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":False}}})
    elif meth=="session/new": send({"jsonrpc":"2.0","id":m["id"],"result":{"sessionId":sid}})
    elif meth=="session/prompt": threading.Thread(target=turn,args=(m["id"],)).start()
    elif meth=="session/cancel":
        for k,ev in list(pending.items()): ev.answer={"outcome":{"outcome":"cancelled"}}; ev.set()
