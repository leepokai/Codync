"""Persistent ACP conversations, with a separate process group per live tool allocation."""
import json
import os
import pathlib
import subprocess
import sys
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
    worker = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(600)' if '--stubborn-workers' in sys.argv else 'import sys; sys.stdin.read()'],
                              stdin=subprocess.PIPE,
                              **(dict(preexec_fn=os.setpgrp) if os.name != 'nt' else {}))
    workers.setdefault(sid, []).append(worker)
    log('allocated', session=sid, worker=worker.pid)


log('started')
for line in sys.stdin:
    req = json.loads(line)
    method, params = req.get('method'), req.get('params', {})
    if method == 'initialize':
        caps = dict(loadSession='--no-load' not in sys.argv,
                    sessionCapabilities={} if '--no-close' in sys.argv else dict(close={}))
        result = dict(agentCapabilities=caps)
        if '--claude' in sys.argv:
            result['agentCapabilities']['_meta'] = dict(claudeCode={})
        answer(req, result)
    elif method in ('session/new', 'session/load'):
        sid = params.get('sessionId') or str(uuid.uuid4())
        if method == 'session/load' and (root / 'fail-load').exists():
            log('load_failed', session=sid)
            answer(req, error=dict(code=-32000, message='fixture cannot load right now'))
            continue
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
    elif method == 'session/set_model':
        sid = params['sessionId']
        failed = root / ('model-failed-' + sid)
        forced = root / 'fail-model'
        if ('--fail-model-once' in sys.argv and not failed.exists()) or forced.exists():
            failed.touch()
            if forced.exists():
                forced.unlink()
            log('model_failed', session=sid)
            answer(req, error=dict(code=-32000, message='transient model selection failure'))
        else:
            log('model_selected', session=sid)
            answer(req)
    elif method == 'session/prompt':
        sid = params['sessionId']
        text = params['prompt'][0]['text']
        sessions[sid].append(text)
        state_file.write_text(json.dumps(sessions))
        log('prompt', session=sid, count=len(sessions[sid]), text=text)
        if 'COMPACT_RESOURCE_SESSION' in text:
            print(json.dumps(dict(jsonrpc='2.0', method='session/update', params=dict(
                sessionId=sid, update=dict(sessionUpdate='compaction_update', compactionId=str(uuid.uuid4()),
                                          status='completed')))), flush=True)
        print(json.dumps(dict(jsonrpc='2.0', method='session/update', params=dict(
            sessionId=sid, update=dict(sessionUpdate='agent_message_chunk',
                                      content=dict(type='text', text='Resource fixture replied.'))))), flush=True)
        answer(req, dict(stopReason='end_turn'))
    elif 'id' in req:
        answer(req)
# A real adapter may leave MCP groups after EOF. The host must reap them before replacement.
log('eof')
