import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Entry } from '../../../shared/models.ts'
import { currentChecklist } from './task-checklist.ts'

const entry = (seq: number, kind: string, turn: number, data: Record<string, unknown> = {}): Entry =>
  ({ id: `e${seq}`, seq, botId: 'b', rev: seq, kind, turn, data, createdAt: 0, updatedAt: 0 }) as Entry

const items = (...statuses: string[]) => statuses.map((status, i) => ({ content: `step ${i + 1}`, status }))

test("the checklist is the latest turn's plan while the bot works or something is open", () => {
  const entries = [entry(1, 'user', 1), entry(2, 'plan', 1, { entries: items('completed', 'in_progress', 'pending') })]
  const list = currentChecklist(entries, true)
  assert.equal(list?.done, 1)
  assert.equal(list?.items.length, 3)
  assert.equal(currentChecklist(entries, false)?.done, 1, 'an open plan stays while the bot rests')
})

test("a finished plan leaves once the bot is idle, and an older turn's plan never shows", () => {
  const done = [entry(1, 'plan', 1, { entries: items('completed', 'completed') })]
  assert.equal(currentChecklist(done, false), null)
  assert.equal(currentChecklist(done, true)?.done, 2, 'still on screen while the bot is finishing')
  const older = [...done, entry(2, 'user', 2), entry(3, 'agent', 2)]
  assert.equal(currentChecklist(older, true), null, 'the next turn wrote no plan')
  assert.equal(currentChecklist([entry(1, 'plan', 1, { entries: [] })], true), null, 'an empty plan')
  assert.equal(currentChecklist([], true), null)
})

test('the last plan of the turn wins', () => {
  const entries = [entry(1, 'plan', 3, { entries: items('pending') }), entry(2, 'plan', 3, { entries: items('completed', 'pending') })]
  assert.equal(currentChecklist(entries, false)?.items.length, 2)
})
