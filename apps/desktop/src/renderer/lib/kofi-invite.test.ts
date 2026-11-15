import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Entry } from '../../shared/models'
import type { ChatItem } from '../views/thread/chat-items'
import { kofiInviteAt } from './kofi-invite.ts'

const item = (id: string, kind: Entry['kind'], createdAt: number, final?: boolean): ChatItem => ({
  id,
  kind: 'entry',
  groupStart: true,
  entry: { id, seq: 0, botId: 'b', rev: 0, kind, turn: 0, data: { text: id, final }, createdAt, updatedAt: 0 } as Entry,
})

const items = [item('old', 'agent', 50, true), item('me', 'user', 100), item('thinking', 'agent', 110), item('reply', 'agent', 120, true), item('later', 'agent', 130, true)]

test('the Ko-fi invitation sits under the first finished reply after the first prompt, in that chat only', () => {
  const due = { kind: 'due', bot: 'b', since: 100 } as const
  assert.equal(kofiInviteAt(due, 'b', items), 'reply')
  assert.equal(kofiInviteAt(due, 'other', items), null)
  assert.equal(kofiInviteAt(due, 'b', items.slice(0, 3)), null)
})

test('the Ko-fi invitation never shows before it is due or once it is done', () => {
  assert.equal(kofiInviteAt({ kind: 'never' }, 'b', items), null)
  assert.equal(kofiInviteAt({ kind: 'done' }, 'b', items), null)
})
