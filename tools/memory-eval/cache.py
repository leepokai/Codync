#!/usr/bin/env python3
"""How well bots' prompt cache holds, read from the harnesses' own session logs.

Claude Code keeps them in ~/.claude/projects, Codex in ~/.codex/sessions. Each session file is one conversation;
each API call in it is counted once. Other harnesses aren't read; the host's `turn usage` log covers any harness
whose adapter reports cached tokens.
Usage: tools/memory-eval/cache.py [workspace-part ...]
A part matches the workspace path with every non-alphanumeric character as "-", the way Claude Code names its
folders (default: the bots of ~/.codync and ~/.codync-dev).
"""
import glob
import json
import os
import re
import sys
from collections import Counter
from datetime import datetime
from statistics import mean, median

# Claude on a subscription writes 1-hour entries; extra usage or an API key gets 5 minutes. The "1-hour writes"
# line shows which one an account got.
LIFE = 3600
PARTS = sys.argv[1:] or ['-codync-bots-', '-codync-dev-bots-']


def wanted(path):
    return any(p in re.sub(r'[^A-Za-z0-9]', '-', path) for p in PARTS)


def gaps(stamps):
    prev = None
    for ts in stamps:
        t = datetime.fromisoformat(ts.replace('Z', '+00:00'))
        yield (t - prev).total_seconds() if prev else None
        prev = t


def claude():
    """Per conversation: (read, total input, 1h writes, writes, model, seconds since the previous call)."""
    sessions = []
    for folder in glob.glob(os.path.expanduser('~/.claude/projects/*')):
        if not wanted(os.path.basename(folder)):
            continue
        seen = set()
        for path in glob.glob(os.path.join(folder, '*.jsonl')):
            raw = []
            for line in open(path, errors='replace'):
                try:
                    rec = json.loads(line)
                except ValueError:
                    continue
                msg = rec.get('message') if isinstance(rec, dict) else None
                if not isinstance(msg, dict) or msg.get('role') != 'assistant' or 'usage' not in msg:
                    continue
                key = msg.get('id') or rec.get('uuid')
                if key in seen:  # a forked session repeats its parent's calls
                    continue
                seen.add(key)
                u = msg['usage']
                read = u.get('cache_read_input_tokens') or 0
                write = u.get('cache_creation_input_tokens') or 0
                total = read + write + (u.get('input_tokens') or 0)
                w1h = (u.get('cache_creation') or {}).get('ephemeral_1h_input_tokens') or 0
                if total:
                    raw.append((read, total, w1h, write, msg.get('model', ''), rec['timestamp']))
            rows = [r[:5] + (g,) for r, g in zip(raw, gaps(r[5] for r in raw))]
            if rows:
                sessions.append(rows)
    return sessions


def codex():
    """The same rows from Codex rollouts. OpenAI caches on its own and charges nothing to write."""
    sessions = []
    for path in glob.glob(os.path.expanduser('~/.codex/sessions/**/rollout-*.jsonl'), recursive=True):
        raw, last_total = [], None
        for line in open(path, errors='replace'):
            try:
                rec = json.loads(line)
            except ValueError:
                continue
            p = rec.get('payload') or {}
            if rec.get('type') == 'session_meta':
                if not wanted(p.get('cwd', '')):
                    break
                continue
            if rec.get('type') != 'event_msg' or p.get('type') != 'token_count' or not p.get('info'):
                continue
            total = p['info']['total_token_usage']['total_tokens']
            if total == last_total:  # the same usage reported again
                continue
            last_total = total
            u = p['info']['last_token_usage']
            if u['input_tokens']:
                raw.append((u['cached_input_tokens'], u['input_tokens'], 0, 0, '', rec['timestamp']))
        rows = [r[:5] + (g,) for r, g in zip(raw, gaps(r[5] for r in raw))]
        if rows:
            sessions.append(rows)
    return sessions


def cost(read, total, w1h, write, model):
    """Anthropic input cost at list multipliers: read 0.1x (0.05x on the 5.5 models), 5-minute write 1.25x, 1-hour 2x."""
    return total - read - write + 1.25 * (write - w1h) + 2 * w1h + (0.05 if '-5-5' in model else 0.1) * read


def pct(x):
    return f'{100 * x:5.1f}%'


def report(name, sessions, priced):
    if not sessions:
        print(f'{name}: no sessions found\n')
        return
    every = [r for s in sessions for r in s]
    warm = [r for r in every if r[5] is not None and r[5] <= LIFE]
    per_every = [sum(r[0] for r in s) / sum(r[1] for r in s) for s in sessions]
    per_warm = [sum(r[0] for r in h) / sum(r[1] for r in h) for s in sessions
                if (h := [r for r in s if r[5] is not None and r[5] <= LIFE])]
    causes = Counter('first call' if r[5] is None else 'after the cache expired' if r[5] > LIFE else 'mid-session miss'
                     for r in every if r[0] < r[1] / 2)
    total = sum(r[1] for r in every)

    def line(label, rows, per):
        by_tokens = sum(r[0] for r in rows) / sum(r[1] for r in rows) if rows else 0
        spread = f'{pct(mean(per)):>11}  {pct(median(per)):>9}' if per else ''
        per_call = pct(mean(r[0] / r[1] for r in rows)) if rows else ''
        return f'{label:<22} {pct(by_tokens):>9}  {per_call:>8}  {spread}'

    print(f'{name}: {len(sessions)} conversations, {len(every)} calls, {total / 1e6:.2f}M input tokens')
    print(f"{'hit rate':<22} {'by tokens':>9}  {'per call':>8}  {'per session':>11}  {'median':>9}")
    print(line('in a conversation', warm, per_warm))
    print(line('every call', every, per_every))
    print(f'cold calls (under half read from cache): {dict(causes)}')
    if priced:
        w1h, write = sum(r[2] for r in every), sum(r[3] for r in every)
        print(f'1-hour writes: {pct(w1h / max(write, 1))} of cache writes; input cost vs. uncached: '
              f'{pct(sum(cost(*r[:5]) for r in every) / total)}')
    print()


report('Claude', claude(), priced=True)
report('Codex', codex(), priced=False)
print('"in a conversation" leaves out each first call and calls more than an hour after the previous one.')
