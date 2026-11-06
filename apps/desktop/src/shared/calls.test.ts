import assert from 'node:assert/strict'
import { test } from 'node:test'
import { callConversation, hidingCalls, parseCallCue, withCallEntries } from './calls.ts'
import type { Entry } from './models.ts'

const entry = (id: string, kind: string, at: number, data: Entry['data']): Entry => ({
  id, seq: at, botId: 'b', rev: 1, kind, turn: 1, data, createdAt: at, updatedAt: at,
})

test("a call hides what it carries, while it lasts too", () => {
  const entries = [
    entry('u1', 'user', 1, { text: 'spoken', call: 'c' }),
    entry('a1', 'agent', 2, { text: 'reply', final: true, call: 'c' }),
    entry('u2', 'user', 3, { text: 'no line yet', call: 'lost' }),
    entry('u3', 'user', 4, { text: 'on this window', call: 'live' }),
    entry('local-n', 'user', 5, { text: "didn't go out", call: 'live', status: 'failed' }),
    entry('f1', 'agent', 6, { text: 'log.txt (3 bytes)', final: true, call: 'c', files: [{ id: 'f', name: 'log.txt', size: 3, sha256: '' }] }),
    entry('n1', 'notice', 7, { text: 'Release plan', callSeconds: 26, call: 'c' }),
  ]
  assert.deepEqual(hidingCalls(entries).map((e) => e.id), ['local-n', 'f1', 'n1'])
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

test("a call's own entries fill in what the loaded chat doesn't hold", () => {
  const line = entry('n1', 'notice', 10, { text: 'Old call', callSeconds: 30, call: 'c' })
  const live = [line]
  const fetched = [
    entry('u1', 'user', 2, { text: 'from before the loaded chat', call: 'c' }),
    entry('a1', 'agent', 3, { text: 'its reply', final: true, call: 'c' }),
  ]
  assert.deepEqual(callConversation(live, line), [])
  const merged = withCallEntries(live, fetched)
  assert.deepEqual(callConversation(merged, line).map((m) => `${m.speaker}: ${m.text}`), ['user: from before the loaded chat', 'bot: its reply'])
  assert.equal(withCallEntries(merged, fetched).length, merged.length, 'each entry once')
})

test("the computer's cues for a call's voice parse by kind; a new kind is passed over", () => {
  assert.deepEqual(parseCallCue({ type: 'call', call: 'c', kind: 'progress', text: 'Mika: Running tests', owner: 'local', seq: 7 }), {
    call: 'c', kind: 'progress', text: 'Mika: Running tests', seq: 7,
  })
  assert.equal(parseCallCue({ type: 'call', call: 'c', kind: 'whisper', text: '?', seq: 8 }), null)
  assert.equal(parseCallCue({ type: 'call', kind: 'reply', text: 'no call', seq: 9 }), null)
})
