import assert from 'node:assert/strict'
import { test } from 'node:test'
import { trackBody, type AnalyticsEvent } from '../../shared/analytics.ts'
import { AnalyticsQueue } from './analytics-queue.ts'

const settle = () => new Promise((r) => setImmediate(r))

test('events from before the host answers go out once it does, in order', async () => {
  const queue = new AnalyticsQueue()
  const sent: AnalyticsEvent[] = []
  queue.track({ event: 'app_opened', at_login: false })
  queue.track({ event: 'onboarding_completed', signed_in: true })
  await settle()
  queue.connect(async (e) => void sent.push(e))
  queue.track({ event: 'screen_viewed', screen: 'chat' })
  await settle()
  assert.deepEqual(sent.map((e) => e.event), ['app_opened', 'onboarding_completed', 'screen_viewed'])
})

test('an unreachable host keeps the event for the next try; a gone host holds them again', async () => {
  const queue = new AnalyticsQueue()
  const sent: string[] = []
  queue.connect(async () => {
    throw new Error('unreachable')
  })
  queue.track({ event: 'window_opened' })
  await settle()
  assert.equal(sent.length, 0)
  queue.connect(async (e) => void sent.push(e.event))
  await settle()
  assert.deepEqual(sent, ['window_opened'])
  queue.connect(null)
  queue.track({ event: 'signed_out' })
  await settle()
  assert.deepEqual(sent, ['window_opened'])
})

test('only the newest events wait past the limit', async () => {
  const queue = new AnalyticsQueue(2)
  const sent: string[] = []
  for (const link of ['docs', 'issues', 'privacy'] as const) queue.track({ event: 'link_opened', link })
  queue.connect(async (e) => void sent.push(e.event === 'link_opened' ? e.link : e.event))
  await settle()
  assert.deepEqual(sent, ['issues', 'privacy'])
})

test('track sends the event name and its properties, from the desktop app', () => {
  assert.deepEqual(trackBody({ event: 'sign_in_failed', provider: 'google', reason: 'timeout' }), {
    event: 'sign_in_failed', provider: 'google', reason: 'timeout', app: 'desktop',
  })
  assert.deepEqual(trackBody({ event: 'window_opened' }), { event: 'window_opened', app: 'desktop' })
})
