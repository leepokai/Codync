import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { UsageProvider, UsageWindow } from '../../shared/models.ts'
import { logins, tightest, tightestLogin } from './usage/usage-logins.ts'

const limit = (percent: number): UsageWindow => ({ id: 'weekly', label: 'Weekly', percent })
const provider = (windows: UsageWindow[], rest: Partial<UsageProvider> = {}): UsageProvider =>
  ({ id: 'claude', name: 'Claude', source: 'test', updatedAt: 1, windows, ...rest })

test('an agent on one login is one unnamed login', () => {
  assert.deepEqual(logins(provider([limit(10)])), [{ id: null, name: null, windows: [limit(10)] }])
})

test('the login nearest its limit stands for the agent', () => {
  const note: UsageWindow = { id: 'cost', label: 'Cost', percent: 99, text: '$9' }
  const p = provider([limit(10), note], {
    defaultLoginProfile: 'me@example.com',
    loginProfiles: [
      { id: 'a', name: 'Work', windows: [limit(90)], updatedAt: 1 },
      { id: 'b', name: 'Side', windows: [], updatedAt: 0 },
    ],
  })
  assert.deepEqual(logins(p).map((l) => l.name), ['me@example.com', 'Work', 'Side'])
  assert.equal(tightestLogin(p).name, 'Work')
  assert.equal(tightest(p)?.percent, 90, 'a note never counts')
  assert.equal(tightestLogin(provider([note], { loginProfiles: [{ id: 'b', name: 'Side', windows: [], updatedAt: 0 }] })).id, null)
})
