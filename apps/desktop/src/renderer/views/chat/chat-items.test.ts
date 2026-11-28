import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import type { Entry } from '../../../shared/models.ts'
import { buildChat, rowIdentity } from './chat-items.ts'
import { entryIdsOf } from '../../lib/shown-window.ts'

/** A bot-to-bot notice the transcript groups with its neighbours from the same peer. */
const exchange = (id: string, seq: number): Entry => ({
  id,
  seq,
  botId: 'egan',
  rev: 1,
  kind: 'notice',
  turn: 0,
  createdAt: 1_000 + seq,
  updatedAt: 1_000 + seq,
  data: { status: 'completed', botMessage: { sourceBotId: 'egan', targetBotId: 'owen', text: `message ${id}` } },
})

test('an older exchange fetched into a group leaves every member id on the rendered row', () => {
  // The reader's row was x1 on its own. The older page adds x0, and buildChat merges both into a group whose primary
  // entry is now x0. The DOM row must still name x1 for the saved place and a quote's reveal to find it.
  const before = buildChat([exchange('x1', 1)]).find((item) => item.kind === 'exchanges')!
  assert.deepEqual(entryIdsOf(before), ['x1'])
  assert.equal(renderToStaticMarkup(createElement('div', rowIdentity(before))), '<div data-entry-id="x1" data-entry-ids="x1"></div>')

  const after = buildChat([exchange('x0', 0), exchange('x1', 1)]).find((item) => item.kind === 'exchanges')!
  assert.equal(after.entry.id, 'x0')
  assert.deepEqual(entryIdsOf(after), ['x0', 'x1'])
  assert.equal(renderToStaticMarkup(createElement('div', rowIdentity(after))), '<div data-entry-id="x0" data-entry-ids="x0 x1"></div>')
})
