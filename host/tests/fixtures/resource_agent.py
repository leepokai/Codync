"""Persistent ACP conversations, each live session holding an MCP-like worker in a process group of its own.

--no-close: no session/close (Cursor). --fail-close: session/close answers an error. --resume-only: session/resume, no
session/load. --stubborn-workers: workers ignore their closed stdin, so only the host's cleanup ends them. A prompt
naming HOLD answers once a `release` file is in the workspace."""
import json
import os
import pathlib
import subprocess
import sys
import time
import uuid

root = pathlib.Path.cwd()
state_file = root / 'resource-sessions.json'
sessions = json.loads(state_file.read_text()) if state_file.exists() else {}
workers = {}


def log(event, **fields):
    with (root / 'resources.jsonl').open('a') as f:
        f.write(json.dumps(dict(event=event, pid=os.getpid(), **fields)) + '\n')


def answer(req, result=None, error=None):
    value = dict(jsonrpc='2.0', id=req['id'])
    value['error' if error else 'result'] = error or result or {}
    print(json.dumps(value), flush=True)


def allocate(sid):
    code = 'import time; time.sleep(600)' if '--stubborn-workers' in sys.argv else 'import sys; sys.stdin.read()'
    group = dict(preexec_fn=os.setpgrp) if os.name != 'nt' else {}
    worker = subprocess.Popen([sys.executable, '-c', code], stdin=subprocess.PIPE, **group)
    workers.setdefault(sid, []).append(worker)
    log('allocated', session=sid, worker=worker.pid)


log('started')
for line in sys.stdin:
    req = json.loads(line)
    method, params = req.get('method'), req.get('params', {})
    if method == 'initialize':
        caps = dict(loadSession='--resume-only' not in sys.argv,
                    sessionCapabilities={} if '--no-close' in sys.argv else dict(close={}))
        if '--resume-only' in sys.argv:
            caps['sessionCapabilities']['resume'] = {}
        answer(req, dict(agentCapabilities=caps))
    elif method in ('session/new', 'session/load', 'session/resume'):
        sid = params.get('sessionId') or str(uuid.uuid4())
        sessions.setdefault(sid, [])
        state_file.write_text(json.dumps(sessions))
        allocate(sid)
        log(method, session=sid)
        answer(req, dict(sessionId=sid) if method == 'session/new' else {})
    elif method == 'session/close':
        if '--fail-close' in sys.argv:
            answer(req, error=dict(code=-32000, message='fixture cannot close'))
        else:
            for worker in workers.pop(params['sessionId'], []):
                worker.kill()
                worker.wait()
            log('closed', session=params['sessionId'])
            answer(req)
    elif method == 'session/prompt':
        sid = params['sessionId']
        text = params['prompt'][0]['text']
        sessions[sid].append(text)
        state_file.write_text(json.dumps(sessions))
        log('prompt', session=sid, count=len(sessions[sid]), text=text)
        while 'HOLD' in text and not (root / 'release').exists():
            time.sleep(0.05)
        print(json.dumps(dict(jsonrpc='2.0', method='session/update', params=dict(
            sessionId=sid, update=dict(sessionUpdate='agent_message_chunk',
                                      content=dict(type='text', text='Resource fixture replied.'))))), flush=True)
        answer(req, dict(stopReason='end_turn'))
    elif 'id' in req:
        answer(req)
# A real adapter may leave its MCP servers behind after EOF: the host's cleanup must end them.
log('eof')
