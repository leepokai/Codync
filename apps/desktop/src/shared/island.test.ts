import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { BotStatus } from './models.ts'
import {
  focusOf, hitRect, hits, islandFrame, islandMode, islandRect, notchOf, othersOf, transitions,
  type IslandBot, type IslandNotice, type IslandPermission, type IslandSummary, type IslandTab,
} from './island.ts'

const bot = (ref: string, status: BotStatus): IslandBot => ({
  ref, name: ref, shape: 'blob', color: 'blue', status, backend: 'Claude Code', folder: 'site', detail: '', steps: [], error: null,
})
const card = (entryId: string): IslandPermission => ({
  ref: 'c/a', entryId, bot: { name: 'a', shape: 'blob', color: 'blue', backend: 'Claude Code' }, headline: 'Wants to run a command', title: 'ls', options: [], answering: null,
})
const summary = (bots: IslandBot[], permissions: IslandPermission[] = []): IslandSummary => ({ bots, permissions, usage: [] })
const state = (over: Partial<{ open: boolean; peek: boolean; tab: IslandTab; deferred: ReadonlySet<string>; notice: IslandNotice | null }> = {}) =>
  ({ open: false, peek: false, tab: 'auto' as IslandTab, deferred: new Set<string>(), notice: null, ...over })

test('closed, the island hides, peeks under the pointer and stays thin while bots work', () => {
  assert.equal(islandMode(summary([bot('a', 'idle')]), state()), 'hidden')
  assert.equal(islandMode(summary([bot('a', 'idle')]), state({ peek: true })), 'peek')
  assert.equal(islandMode(summary([bot('a', 'working')]), state()), 'compact')
  assert.equal(islandMode(summary([bot('a', 'needsInput')]), state()), 'compact')
})

test('open, it shows what needs you first, then the bots', () => {
  assert.equal(islandMode(summary([bot('a', 'working')]), state({ open: true })), 'overview')
  assert.equal(islandMode(summary([bot('a', 'idle')]), state({ open: true })), 'empty')
  assert.equal(islandMode(summary([bot('a', 'idle')]), state({ open: true, notice: { kind: 'finished', ref: 'a' } })), 'finished')
  assert.equal(islandMode(summary([bot('a', 'idle')]), state({ open: true, tab: 'bots', notice: { kind: 'finished', ref: 'a' } })), 'empty')
  assert.equal(islandMode(summary([bot('a', 'working')]), state({ open: true, tab: 'message' })), 'message')
  assert.equal(islandMode(summary([bot('a', 'working')], [card('e1')]), state({ open: true, tab: 'usage' })), 'usage')
})

test('an approval opens the island until it is put off, and opening shows it again', () => {
  const s = summary([bot('a', 'needsInput')], [card('e1')])
  assert.equal(islandMode(s, state()), 'approval')
  assert.equal(islandMode(s, state({ deferred: new Set(['e1']) })), 'compact')
  assert.equal(islandMode(s, state({ open: true, deferred: new Set(['e1']) })), 'approval')
  assert.equal(islandMode(s, state({ open: true, tab: 'bots', deferred: new Set(['e1']) })), 'overview')
  // A new card arrives while the first is put off.
  assert.equal(islandMode(summary([], [card('e1'), card('e2')]), state({ deferred: new Set(['e1']) })), 'approval')
})

test('a bot that stops working is done, or failed when it ended in an error', () => {
  const before = new Map<string, string>([['a', 'working'], ['b', 'idle'], ['c', 'needsInput'], ['d', 'working']])
  const after = [bot('a', 'idle'), bot('b', 'idle'), bot('c', 'error'), bot('d', 'working'), bot('e', 'idle')]
  assert.deepEqual(transitions(before, after), [{ kind: 'finished', ref: 'a' }, { kind: 'failed', ref: 'c' }])
})

test('the big slot goes to the bot that needs you, the rest to the side', () => {
  const bots = [bot('a', 'idle'), bot('b', 'working'), bot('c', 'needsInput'), bot('d', 'idle'), bot('e', 'working')]
  assert.equal(focusOf(bots)?.ref, 'c')
  assert.deepEqual(othersOf(bots, focusOf(bots), 3).map((b) => b.ref), ['b', 'e', 'a'])
  assert.equal(focusOf([]), null)
})

test('the notch is the screen width minus the menu bar on both sides', () => {
  const screens = [
    { x: 0, width: 1512, top: 32, left: 663.5, right: 663.5 },
    { x: 1512, width: 2560, top: 0, left: 0, right: 0 },
  ]
  assert.deepEqual(notchOf(screens, { x: 0, width: 1512 }), { width: 185, height: 32 })
  assert.equal(notchOf(screens, { x: 1512, width: 2560 }), null)
  assert.equal(notchOf([], { x: 0, width: 1512 }), null)
})

test('shapes follow the notch, and a plain top edge keeps a thin strip that wakes it', () => {
  const notch = { notch: { width: 185, height: 32 } }
  assert.deepEqual(islandFrame('hidden', notch), { width: 185, height: 32, radius: 14 })
  assert.equal(islandFrame('peek', notch).width, 185 + 64)
  assert.equal(islandFrame('compact', notch).width, 185 + 104)
  // Every open view has one size: switching tabs never moves the shape.
  for (const mode of ['overview', 'empty', 'approval', 'finished', 'failed', 'message', 'usage'] as const) {
    assert.deepEqual(islandFrame(mode, notch), { width: 640, height: 196, radius: 30 })
  }
  // A taller notch pushes the content down.
  assert.equal(islandFrame('approval', { notch: { width: 200, height: 38 } }).height, 202)
  const edge = islandRect(islandFrame('hidden', { notch: null }), 720)
  assert.equal(hits(edge, 360, 0), true)
  assert.equal(hits(edge, 360, 12), false)
  assert.equal(hits(edge, 100, 0), false)
  // Linux shapes the window to this: the strip under the top edge, never above the window.
  assert.deepEqual(hitRect(edge), { x: 272, y: 0, width: 176, height: 8 })
  assert.deepEqual(hitRect({ x: 0, y: 0, width: 0, height: 0 }), { x: 0, y: 0, width: 0, height: 0 })
})
