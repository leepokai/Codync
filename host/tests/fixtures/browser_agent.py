"""ACP fixture exercising Codync's isolated browser through the real computer MCP."""
import json
import os
import re
import subprocess
import sys
import time
import traceback
from pathlib import Path

sessions = {}


def send(value):
    print(json.dumps(value), flush=True)


def call_tool(request, name, arguments, checkpoint):
    reply = request('tools/call', {'name': name, 'arguments': arguments})
    progress = {'tool': name, 'keys': sorted(reply), 'isError': reply.get('isError', False), 'contentType': type(reply.get('content')).__name__}
    checkpoint('tool-result', progress)
    text = reply['content'][0]['text']
    if reply.get('isError'):
        return False, text
    return True, json.loads(text)


def binding(start):
    return {
        'operationId': start['target']['operationId'],
        'browserId': start['target']['browserId'],
        'tabId': start['target']['tabId'],
        'generation': start['target']['generation'],
    }


def prompt(message):
    params = message['params']
    session = sessions[params['sessionId']]
    cwd = Path(session['cwd'])
    text = params['prompt'][0]['text']
    match = re.search(r'https?://[^\s]+', text)
    assert match, f'supply a page URL: {text}'
    url = match.group(0).rstrip('.,)')
    tool = next(server for server in session['mcpServers'] if server['name'] == 'computer')
    results = {
        'mode': 'concurrent' if 'browser-concurrent' in text
        else 'queued-takeover' if 'browser-queued-takeover' in text
        else 'blocked-stop' if 'browser-blocked-stop' in text
        else 'start-stop' if 'browser-start-stop' in text
        else 'setup-fail' if 'browser-setup-fail' in text
        else 'closed-stop' if 'browser-closed-stop' in text
        else 'happy' if 'browser-happy' in text
        else 'takeover' if 'browser-takeover' in text
        else 'stop'
    }
    progress = {'stage': 'spawn-mcp'}

    def checkpoint(stage, detail=None):
        progress['stage'] = stage
        progress['at'] = time.time()
        if detail is not None:
            progress['detail'] = detail
        (cwd / 'browser-progress.json').write_text(json.dumps(progress, indent=2))

    checkpoint('spawn-mcp')
    with open(cwd / 'browser-mcp.log', 'w', encoding='utf-8') as mcp_log, subprocess.Popen(
        [tool['command'], *tool['args']],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=mcp_log,
        text=True,
        env=os.environ.copy(),
    ) as mcp:
        sequence = 0

        def request(method, arguments):
            nonlocal sequence
            sequence += 1
            checkpoint(f'request:{method}')
            mcp.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': sequence, 'method': method, 'params': arguments}) + '\n')
            mcp.stdin.flush()
            line = mcp.stdout.readline()
            assert line, f'MCP exited before replying to {method}'
            checkpoint(f'line:{method}', {'line': line[:2048]})
            message = json.loads(line)
            checkpoint(f'parsed:{method}', {'keys': sorted(message), 'error': message.get('error')})
            reply = message['result']
            checkpoint(f'reply:{method}')
            return reply

        request('initialize', {
            'protocolVersion': '2025-06-18',
            'capabilities': {},
            'clientInfo': {'name': 'browser-test', 'version': '1'},
        })
        names = {item['name'] for item in request('tools/list', {})['tools']}
        needed = {'browser_start', 'browser_navigate', 'browser_click', 'browser_type', 'browser_screenshot', 'browser_stop'}
        assert needed <= names, names
        checkpoint('tools-ready')

        def tool(name, arguments):
            checkpoint(f'tool:{name}:request')
            ok, value = call_tool(request, name, arguments, checkpoint)
            checkpoint(f'tool:{name}:reply', {'ok': ok, 'error': None if ok else value})
            return ok, value

        if results['mode'] == 'concurrent':
            def send_start():
                nonlocal sequence
                sequence += 1
                request_id = sequence
                mcp.stdin.write(json.dumps({
                    'jsonrpc': '2.0',
                    'id': request_id,
                    'method': 'tools/call',
                    'params': {'name': 'browser_start', 'arguments': {'url': url}},
                }) + '\n')
                mcp.stdin.flush()
                return request_id

            pending_ids = {send_start(), send_start()}
            replies = {}
            while pending_ids:
                line = mcp.stdout.readline()
                assert line, 'MCP exited before replying to concurrent browser_start'
                message = json.loads(line)
                if message.get('id') in pending_ids:
                    pending_ids.remove(message['id'])
                    replies[message['id']] = message['result']
            starts = []
            for reply in replies.values():
                assert not reply.get('isError'), reply
                starts.append(json.loads(reply['content'][0]['text']))
            starts.sort(key=lambda item: item['reused'])
            assert starts[0]['reused'] is False and starts[1]['reused'] is True, starts
            assert binding(starts[0]) == binding(starts[1]), starts
            results['concurrentStarts'] = starts
            ok, closed = tool('browser_stop', binding(starts[0]))
            assert ok, closed
            results['stop'] = closed
            (cwd / 'browser-results.json').write_text(json.dumps(results, indent=2))
            checkpoint('results-written')
            mcp.stdin.close()
            send({
                'jsonrpc': '2.0',
                'method': 'session/update',
                'params': {
                    'sessionId': params['sessionId'],
                    'update': {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text': 'Concurrent browser start reused one tab.'}},
                },
            })
            return {'stopReason': 'end_turn'}

        ok, start = tool('browser_start', {'url': url})
        if results['mode'] == 'setup-fail':
            assert not ok and 'creating the extension browser evidence directory' in start, (ok, start)
            results['startError'] = start
            (cwd / 'browser-results.json').write_text(json.dumps(results, indent=2))
            checkpoint('results-written')
            mcp.stdin.close()
            send({
                'jsonrpc': '2.0',
                'method': 'session/update',
                'params': {
                    'sessionId': params['sessionId'],
                    'update': {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text': 'Browser setup failure observed.'}},
                },
            })
            return {'stopReason': 'end_turn'}
        if results['mode'] == 'start-stop':
            assert not ok and ('cancelled' in start or 'no longer running' in start), (ok, start)
            results['startError'] = start
            checkpoint('start-cancelled')
            (cwd / 'browser-results.json').write_text(json.dumps(results, indent=2))
            checkpoint('results-written')
            mcp.stdin.close()
            send({
                'jsonrpc': '2.0',
                'method': 'session/update',
                'params': {
                    'sessionId': params['sessionId'],
                    'update': {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text': 'Browser start cancelled.'}},
                },
            })
            return {'stopReason': 'end_turn'}
        assert ok, start
        bound = binding(start)
        results['start'] = start
        checkpoint('start-complete', {'mode': results['mode']})

        if results['mode'] == 'queued-takeover':
            def send_tool(name, arguments):
                nonlocal sequence
                sequence += 1
                request_id = sequence
                mcp.stdin.write(json.dumps({
                    'jsonrpc': '2.0',
                    'id': request_id,
                    'method': 'tools/call',
                    'params': {'name': name, 'arguments': arguments},
                }) + '\n')
                mcp.stdin.flush()
                return request_id

            nav_id = send_tool('browser_navigate', dict(bound, url=url))
            time.sleep(0.25)
            click_id = send_tool('browser_click', dict(bound, selector='#count'))
            pending_ids = {nav_id, click_id}
            replies = {}
            while pending_ids:
                line = mcp.stdout.readline()
                assert line, 'MCP exited before replying to queued takeover requests'
                message = json.loads(line)
                if message.get('id') in pending_ids:
                    pending_ids.remove(message['id'])
                    replies[message['id']] = message['result']
            nav_reply = replies[nav_id]
            click_reply = replies[click_id]
            results['slowNavigateIsError'] = bool(nav_reply.get('isError', False))
            results['slowNavigateText'] = nav_reply['content'][0]['text']
            results['queuedClickIsError'] = bool(click_reply.get('isError', False))
            results['queuedClickText'] = click_reply['content'][0]['text']
            assert results['queuedClickIsError'] is True, click_reply
            ok, closed = tool('browser_stop', bound)
            assert ok, closed
            results['stop'] = closed
            (cwd / 'browser-results.json').write_text(json.dumps(results, indent=2))
            checkpoint('results-written')
            mcp.stdin.close()
            send({
                'jsonrpc': '2.0',
                'method': 'session/update',
                'params': {
                    'sessionId': params['sessionId'],
                    'update': {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text': 'Queued click aborted on takeover.'}},
                },
            })
            return {'stopReason': 'end_turn'}

        if results['mode'] == 'blocked-stop':
            (cwd / 'browser-active.json').write_text(json.dumps(bound))
            ok, nav = tool('browser_navigate', dict(bound, url=url))
            results['navigateOk'] = ok
            results['navigateResult'] = nav
            (cwd / 'browser-results.json').write_text(json.dumps(results, indent=2))
            checkpoint('results-written')
            mcp.stdin.close()
            send({
                'jsonrpc': '2.0',
                'method': 'session/update',
                'params': {
                    'sessionId': params['sessionId'],
                    'update': {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text': 'Blocked navigate ended.'}},
                },
            })
            return {'stopReason': 'end_turn'}

        if results['mode'] == 'closed-stop':
            ok, closed = tool('browser_stop', bound)
            assert ok and closed['alreadyClosed'] is True, (ok, closed)
            ok, restarted = tool('browser_start', {'url': url})
            assert ok and restarted['reused'] is False, (ok, restarted)
            restarted_bound = binding(restarted)
            assert restarted_bound['browserId'] != bound['browserId'], (restarted, start)
            ok, final_closed = tool('browser_stop', restarted_bound)
            assert ok and final_closed['alreadyClosed'] is False, (ok, final_closed)
            results['closedStop'] = closed
            results['restarted'] = restarted
            results['finalStop'] = final_closed
            (cwd / 'browser-results.json').write_text(json.dumps(results, indent=2))
            checkpoint('results-written')
            mcp.stdin.close()
            send({
                'jsonrpc': '2.0',
                'method': 'session/update',
                'params': {
                    'sessionId': params['sessionId'],
                    'update': {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text': 'Closed browser session replaced.'}},
                },
            })
            return {'stopReason': 'end_turn'}

        if results['mode'] == 'happy':
            ok, reused = tool('browser_start', {'url': url})
            assert ok and reused['reused'] is True, reused
            assert binding(reused) == bound, (reused, bound)
            results['reusedStart'] = reused
            checkpoint('reuse-complete')

            stale = dict(bound, generation='00000000-0000-0000-0000-000000000000')
            ok, stale_error = tool('browser_click', dict(stale, selector='#count'))
            assert not ok and 'browser target changed' in stale_error, (ok, stale_error)
            results['staleError'] = stale_error
            checkpoint('stale-complete')

            ok, click = tool('browser_click', dict(bound, selector='#count'))
            assert ok, click
            results['click'] = click
            checkpoint('click-complete')
            ok, typed = tool('browser_type', dict(bound, selector='#name', text='Codync'))
            assert ok, typed
            results['type'] = typed
            checkpoint('type-complete')
            ok, shot = tool('browser_screenshot', bound)
            assert ok, shot
            results['screenshot'] = shot
            checkpoint('screenshot-complete')
            ok, closed = tool('browser_stop', bound)
            assert ok, closed
            results['stop'] = closed
            checkpoint('stop-complete')
            ok, post_stop_error = tool('browser_type', dict(bound, selector='#name', text='must-not-land'))
            assert not ok and 'no longer running' in post_stop_error, (ok, post_stop_error)
            results['postStopError'] = post_stop_error
            checkpoint('post-stop-complete')
        else:
            (cwd / 'browser-active.json').write_text(json.dumps(bound))
            time.sleep(2)
            if results['mode'] == 'takeover':
                ok, error = tool('browser_click', dict(bound, selector='#count'))
                assert not ok, ok
                results['guardError'] = error
            else:
                ok, error = tool('browser_type', dict(bound, selector='#name', text='must-not-land'))
                assert not ok, ok
                results['cancelError'] = error

        (cwd / 'browser-results.json').write_text(json.dumps(results, indent=2))
        checkpoint('results-written')
        mcp.stdin.close()
    send({
        'jsonrpc': '2.0',
        'method': 'session/update',
        'params': {
            'sessionId': params['sessionId'],
            'update': {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text': 'Browser operation complete.'}},
        },
    })
    return {'stopReason': 'end_turn'}


for line in sys.stdin:
    message = json.loads(line)
    method = message.get('method')
    if method == 'initialize':
        result = {'protocolVersion': 1, 'agentCapabilities': {'loadSession': True}}
    elif method in ['session/new', 'session/load']:
        session_id = message['params'].get('sessionId', f'browser-test-{len(sessions)}')
        sessions[session_id] = message['params']
        result = {'sessionId': session_id}
    elif method == 'session/prompt':
        try:
            result = prompt(message)
        except Exception as error:
            cwd = Path(sessions[message['params']['sessionId']]['cwd'])
            (cwd / 'browser-error.json').write_text(
                json.dumps({'type': type(error).__name__, 'message': str(error), 'traceback': traceback.format_exc()}, indent=2),
                encoding='utf-8',
            )
            raise
    else:
        continue
    send({'jsonrpc': '2.0', 'id': message['id'], 'result': result})
