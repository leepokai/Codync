import assert from 'node:assert/strict'
import { test } from 'node:test'
import { callConversation, hidingCalls } from './calls.ts'
import type { Entry } from './models.ts'

const entry = (id: string, kind: string, at: number, data: Entry['data']): Entry => ({
  id, seq: at, botId: 'b', rev: 1, kind, turn: 1, data, createdAt: at, updatedAt: at,
})

test("finished and live calls hide what they carried", () => {
  const entries = [
    entry('u1', 'user', 1, { text: 'spoken', call: 'c' }),
    entry('a1', 'agent', 2, { text: 'reply', final: true, call: 'c' }),
    entry('u2', 'user', 3, { text: 'no line yet', call: 'lost' }),
    entry('u3', 'user', 4, { text: 'on this window', call: 'live' }),
    entry('local-n', 'user', 5, { text: "didn't go out", call: 'live', status: 'failed' }),
    entry('n1', 'notice', 6, { text: 'Release plan', callSeconds: 26, call: 'c' }),
  ]
  assert.deepEqual(hidingCalls(entries, new Set(['live'])).map((e) => e.id), ['u2', 'local-n', 'n1'])
})

test("a call's conversation merges its transcript with what it carried", () => {
  const line = entry('n1', 'notice', 10, { text: 'Git', call: 'c', transcript: [{ from: 'user', text: '看一下 git', at: 1 }, { from: 'voice', text: 'Clean.', at: 9 }] })
  const entries = [
    entry('u1', 'user', 2, { text: 'check git', call: 'c', from: 'voice' }),
    entry('a1', 'agent', 3, { text: 'Working tree clean', final: true, call: 'c' }),
    entry('t1', 'agent', 4, { text: 'narration', call: 'c' }),
    entry('x1', 'user', 5, { text: 'typed' }),
    line,
  ]
  assert.deepEqual(callConversation(entries, line).map((m) => `${m.speaker}: ${m.text}`), [
    'user: 看一下 git', 'voiceToBot: check git', 'bot: Working tree clean', 'voice: Clean.',
  ])
})
