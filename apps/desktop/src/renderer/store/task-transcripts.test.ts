import assert from 'node:assert/strict'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { Entry } from '@shared/models'

registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('@shared/')) return nextResolve(new URL(`../../shared/${specifier.slice(8)}.ts`, import.meta.url).href, context)
  return nextResolve(specifier, context)
} })

const entry = (id: string, seq: number, data: Entry['data'] = { text: id }): Entry => ({ id, seq, botId: 'task-1', rev: seq, kind: 'user', turn: 1, data, createdAt: 0, updatedAt: 0 })
const unsent = (nonce: string) => entry(`local-${nonce}`, Number.MAX_SAFE_INTEGER, { text: nonce, clientNonce: nonce, status: 'sending' })

test("a closed thread keeps the messages still on their way, until the host's own arrive", async () => {
  const { TaskTranscripts } = await import('./task-transcripts.ts')
  const transcripts = new TaskTranscripts()
  const ids = () => transcripts.of('task-1').map((e) => e.id)

  const close = transcripts.watch('task-1')
  transcripts.merge('task-1', [entry('a', 1), unsent('n1')])
  close()
  assert.deepEqual(ids(), ['local-n1'], 'only the unsent message stays')
  transcripts.merge('task-1', [entry('e2', 2, { text: 'n1', clientNonce: 'n1' })])
  assert.deepEqual(ids(), [], "the host's entry took its place, and nothing is held any more")

  const closeAgain = transcripts.watch('task-1')
  transcripts.merge('task-1', [entry('b', 3)])
  closeAgain()
  assert.deepEqual(ids(), [], 'nothing unsent: nothing held')
  transcripts.merge('task-1', [entry('c', 4)])
  assert.deepEqual(ids(), [], 'nobody watches it: nothing more is kept')
})
