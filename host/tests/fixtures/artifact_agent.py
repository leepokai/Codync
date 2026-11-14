"""ACP fixture publishing artifacts through the real built-in stdio chat MCP."""
import json
import os
import subprocess
import sys
from pathlib import Path

sessions = {}

def send(value):
    print(json.dumps(value), flush=True)

def prompt(message):
    params = message['params']
    session = sessions[params['sessionId']]
    cwd = Path(session['cwd'])
    text = params['prompt'][0]['text']
    (cwd / 'last-prompt.txt').write_text(text)
    tool = next(server for server in session['mcpServers'] if server['name'] == 'chat')
    with subprocess.Popen([tool['command'], *tool['args']], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, env=os.environ.copy()) as mcp:
        sequence = 0
        def request(method, arguments):
            nonlocal sequence
            sequence += 1
            mcp.stdin.write(json.dumps({'jsonrpc':'2.0', 'id':sequence, 'method':method, 'params':arguments}) + '\n')
            mcp.stdin.flush()
            return json.loads(mcp.stdout.readline())['result']
        request('initialize', {'protocolVersion':'2025-06-18', 'capabilities':{}, 'clientInfo':{'name':'artifact-test','version':'1'}})
        names = [tool['name'] for tool in request('tools/list', {})['tools']]
        assert 'publish_artifact' in names and 'list_artifacts' in names
        def call(name, arguments):
            return request('tools/call', {'name':name, 'arguments':arguments})
        results = []
        if 'make-artifact' in text:
            (cwd / 'report.html').write_text('<h1>Weekly report</h1>')
            results.append(call('publish_artifact', {'path':'report.html', 'title':'Weekly report'}))
            results.append(call('publish_artifact', {'path':'report.html'}))
        elif 'update-artifact' in text:
            artifact = json.loads(call('list_artifacts', {})['content'][0]['text'])['artifacts'][0]
            html = Path(artifact['path']).read_text() + '<p>Updated</p>'
            (cwd / 'report.html').write_text(html)
            results.append(call('publish_artifact', {'path':'report.html', 'artifact':artifact['id']}))
        elif 'share-file' in text:
            (cwd / 'notes.txt').write_text('notes')
            results.append(call('send_file', {'path':'notes.txt'}))
        (cwd / 'artifact-results.json').write_text(json.dumps(results))
        mcp.stdin.close()
    send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':params['sessionId'],'update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'Done.'}}}})
    return {'stopReason':'end_turn'}

for line in sys.stdin:
    message = json.loads(line)
    method = message.get('method')
    if method == 'initialize':
        result = {'protocolVersion':1,'agentCapabilities':{'loadSession':True}}
    elif method in ['session/new', 'session/load']:
        session_id = message['params'].get('sessionId', 'artifact-test-' + str(len(sessions)))
        sessions[session_id] = message['params']
        result = {'sessionId':session_id}
    elif method == 'session/prompt':
        result = prompt(message)
    else:
        continue
    send({'jsonrpc':'2.0','id':message['id'],'result':result})
