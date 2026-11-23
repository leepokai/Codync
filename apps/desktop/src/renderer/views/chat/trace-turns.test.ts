import assert from 'node:assert/strict'
import { test } from 'node:test'
import { nestedSteps, runningTurn, scrolledToEnd, subagentLine, traceTurns, workingSteps } from './trace-turns.ts'
import type { Entry, EntryData } from '../../../shared/models.ts'

const entry = (id: string, turn: number, kind = 'agent', status?: string, data: EntryData = {}): Entry => ({
  id, seq: 0, botId: 'b', rev: 1, kind, turn, data: { text: id, status, ...data }, createdAt: 0, updatedAt: 0,
})

test('the working steps are the running turn’s work, without what the chat or memory already show', () => {
  const entries = [
    entry('old', 1, 'tool'),
    entry('u2', 2, 'user'),
    entry('thinking', 2, 'thought'),
    entry('blank', 2, 'thought', undefined, { text: '  ' }),
    entry('says', 2, 'agent'),
    entry('terminal', 2, 'tool', 'in_progress', { bookkeeping: false }),
    entry('memory', 2, 'tool', 'completed', { bookkeeping: true }),
    entry('unflagged', 2, 'tool', 'completed'),
    entry('plan', 2, 'plan'),
    entry('reply', 2, 'agent', undefined, { final: true }),
    entry('card', 2, 'permission'),
    entry('pass', 2, 'agent', undefined, { text: '(pass)' }),
    entry('q', 3, 'user', 'queued'),
  ]
  assert.deepEqual(workingSteps(entries).map((e) => e.id), ['thinking', 'says', 'terminal', 'plan'])
})

test('turns group in turn order and keep their entries in order', () => {
  const turns = traceTurns([entry('u2', 2, 'user'), entry('u1', 1, 'user'), entry('a1', 1), entry('a2', 2)])
  assert.deepEqual(turns.map(([turn, es]) => [turn, es.map((e) => e.id)]), [[1, ['u1', 'a1']], [2, ['u2', 'a2']]])
})

test('the running turn is the newest one, not a queued message waiting for the next', () => {
  const entries = [entry('u1', 1, 'user'), entry('t1', 1, 'thought'), entry('q', 2, 'user', 'queued')]
  assert.equal(runningTurn(entries, true), 1)
  assert.equal(runningTurn(entries, false), undefined)
})

test('the trace follows new steps only while scrolled to the end', () => {
  assert.equal(scrolledToEnd({ scrollHeight: 1000, scrollTop: 600, clientHeight: 400 }), true)
  assert.equal(scrolledToEnd({ scrollHeight: 1000, scrollTop: 595, clientHeight: 400 }), true)
  assert.equal(scrolledToEnd({ scrollHeight: 1000, scrollTop: 500, clientHeight: 400 }), false)
})

test('a subagent’s steps sit under its launch in its turn, even when other work came between them', () => {
  const tool = (id: string, data: EntryData = {}) => entry(id, 1, 'tool', 'completed', { toolCallId: id, ...data })
  const steps = [
    tool('a'),
    tool('b'),
    tool('a1', { parent: 'a' }),
    tool('b1', { parent: 'b' }),
    tool('a2', { parent: 'a' }),
    tool('lost', { parent: 'gone' }),
    tool('self', { parent: 'self' }),
    entry('later', 2, 'tool', 'completed', { toolCallId: 'later', parent: 'a' }),
  ]
  assert.deepEqual(nestedSteps(steps).map(({ entry, depth }) => [entry.id, depth]), [
    ['a', 0], ['a1', 1], ['a2', 1], ['b', 0], ['b1', 1], ['lost', 0], ['self', 0], ['later', 0],
  ])
})

test('a subagent’s launch reads its kind, tokens and time once finished, or that it runs in the background', () => {
  const tr = (key: string) => key
  assert.equal(subagentLine({ kind: 'general-purpose', background: false }, tr), null)
  assert.equal(subagentLine({ kind: 'general-purpose', tokens: 42310, durationMs: 6287 }, tr), 'general-purpose · 42.3k tokens · 6 s')
  assert.equal(subagentLine({ kind: 'Explore', tokens: 812, durationMs: 125000 }, tr), 'Explore · 812 tokens · 2 min')
  assert.equal(subagentLine({ kind: 'general-purpose', background: true }, tr), 'general-purpose · in background')
  assert.equal(subagentLine(undefined, tr), null)
  const tokens = (n: number) => subagentLine({ tokens: n }, tr)
  assert.deepEqual([1150, 2000, 123_456, 1_200_000].map(tokens), ['1.2k tokens', '2k tokens', '123k tokens', '1.2M tokens'])
})
