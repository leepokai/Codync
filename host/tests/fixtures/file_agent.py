"""ACP fixture using the real built-in stdio chat MCP, including resumed sessions. `share-image` shares a real PNG."""
import json
import os
import subprocess
import struct
import sys
import zlib
from pathlib import Path

sessions = {}

def send(value):
    print(json.dumps(value), flush=True)

def png(width, height):
    """A solid green `width` x `height` PNG."""
    chunk = lambda kind, data: struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    rows = b''.join(b'\0' + b'\x33\xaa\x77' * width for _ in range(height))
    header = struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b'')

def prompt(message):
    params = message['params']
    session = sessions[params['sessionId']]
    tool = next(server for server in session['mcpServers'] if server['name'] == 'chat')
    with subprocess.Popen([tool['command'], *tool['args']], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, env=os.environ.copy()) as mcp:
        sequence = 0
        def request(method, arguments):
            nonlocal sequence
            sequence += 1
            mcp.stdin.write(json.dumps({'jsonrpc':'2.0', 'id':sequence, 'method':method, 'params':arguments}) + '\n')
            mcp.stdin.flush()
            return json.loads(mcp.stdout.readline())['result']
        request('initialize', {'protocolVersion':'2025-06-18', 'capabilities':{}, 'clientInfo':{'name':'file-test','version':'1'}})
        tools = request('tools/list', {})['tools']
        assert any(tool['name'] == 'send_file' for tool in tools)
        text = params['prompt'][0]['text']
        results = []
        if 'share-image' in text:
            Path(session['cwd'], 'chart.png').write_bytes(png(400, 300))
            results.append(request('tools/call', {'name':'send_file', 'arguments':{'path':'chart.png'}}))
        elif 'invalid-files' in text:
            for path in ['missing', '.', 'oversized']:
                results.append(request('tools/call', {'name':'send_file', 'arguments':{'path':path}}))
        elif 'spoken' in text:
            for path in ['note.m4a', 'report.pdf']:
                results.append(request('tools/call', {'name':'send_file', 'arguments':{'path':path, 'text':'Here is the **plan**.'}}))
        else:
            paths = ['binary.dat', '.hidden', 'empty', 'report.pdf']
            for path in paths:
                results.append(request('tools/call', {'name':'send_file', 'arguments':{'path':path}}))
        Path(session['cwd'], 'file-results.json').write_text(json.dumps(results))
        mcp.stdin.close()
    if 'files-only' not in text:
        send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':params['sessionId'],'update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'The files are ready.'}}}})
    return {'stopReason':'end_turn'}

for line in sys.stdin:
    message = json.loads(line)
    method = message.get('method')
    if method == 'initialize':
        result = {'protocolVersion':1,'agentCapabilities':{'loadSession':True}}
    elif method in ['session/new', 'session/load']:
        session_id = message['params'].get('sessionId', 'file-test-' + str(len(sessions)))
        sessions[session_id] = message['params']
        if method == 'session/load': Path(message['params']['cwd'], 'resumed').touch()
        result = {'sessionId':session_id}
    elif method == 'session/prompt':
        result = prompt(message)
    else:
        continue
    send({'jsonrpc':'2.0','id':message['id'],'result':result})
