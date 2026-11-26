import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { BotStatus, Entry } from './models.ts'
import {
  focusOf, hitRect, hits, inTurn, isActive, islandFrame, islandLastError, islandMode, islandRect, islandReply,
  islandTurnSteps, nextTurnStates, notchOf, othersOf, stopsDictation, transitions,
  type BotTurnState, type IslandBot, type IslandNotice, type IslandPermission, type IslandSummary, type IslandTab,
} from './island.ts'

/** `status` is the chat's; `background`: its box holds work, so outside its chat it works. */
const bot = (ref: string, status: BotStatus, background = false, extra: Partial<IslandBot> = {}): IslandBot => ({
  ref, name: ref, shape: 'blob', color: 'blue', status: background && !inTurn(status) ? 'working' : status, chat: status,
  backend: 'Claude Code', folder: 'site', detail: '', steps: [], error: null, reply: null, errorId: null, ...extra,
})
const turn = (chat: string, extra: Partial<BotTurnState> = {}): BotTurnState => ({
  chat, away: false, baselineReplyId: null, baselineErrorId: null, ...extra,
})
const entry = (id: string, kind: string, data: Entry['data'], createdAt = 100): Entry => ({
  id, seq: 1, botId: 'a', rev: 1, kind, turn: 1, data, createdAt, updatedAt: createdAt,
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

test('a bot that stops working is done only with a fresh own-chat reply, or failed on error', () => {
  const before = new Map<string, BotTurnState>([
    ['a', turn('working', { baselineReplyId: 'e1' })],
    ['b', turn('idle')],
    ['c', turn('needsInput', { baselineErrorId: 'err0' })],
    ['d', turn('working')],
  ])
  const after = [
    bot('a', 'idle', false, { reply: { id: 'e2', text: 'New reply' } }),
    bot('b', 'idle'),
    bot('c', 'error', false, { error: 'Agent crashed', errorId: 'err1' }),
    bot('d', 'working'),
    bot('e', 'idle'),
  ]
  assert.deepEqual(transitions(before, after), [
    { kind: 'finished', ref: 'a', text: 'New reply' },
    { kind: 'failed', ref: 'c', text: 'Agent crashed' },
  ])
})

test('a stopped turn stays quiet even after sending a reply, and the next successful turn notifies', () => {
  const sent = { id: 'e1', text: 'Sent before Stop' }
  let states = nextTurnStates(null, [bot('a', 'working', false, { completion: null })])
  const stopped = [bot('a', 'idle', false, { reply: sent, completion: null })]
  assert.deepEqual(transitions(states, stopped), [])
  states = nextTurnStates(states, stopped)

  const finished = [bot('a', 'idle', false, { completion: { id: 'e2', text: 'New reply' } })]
  assert.deepEqual(transitions(states, finished), [{ kind: 'finished', ref: 'a', text: 'New reply' }])
})

test('a queued turn failing in the same summary supersedes the earlier completion', () => {
  const completion = { id: 'e1', text: 'Earlier reply' }
  const failed = [bot('a', 'error', false, { completion, error: 'Queued turn failed', errorId: 'err-new' })]
  for (const previous of ['working', 'idle'] as const) {
    const states = nextTurnStates(null, [bot('a', previous, false, { completion: null, errorId: 'err-old' })])
    assert.deepEqual(transitions(states, failed), [{ kind: 'failed', ref: 'a', text: 'Queued turn failed' }])
    const failedStates = nextTurnStates(states, failed)
    assert.deepEqual(transitions(failedStates, failed), [], 'the same failure is shown once')
    const working = [bot('a', 'working', false, { completion, errorId: 'err-new' })]
    assert.deepEqual(transitions(failedStates, working), [], 'the superseded reply stays quiet')
    const next = [bot('a', 'idle', false, { completion: { id: 'e2', text: 'New reply' } })]
    assert.deepEqual(transitions(nextTurnStates(failedStates, working), next), [{ kind: 'finished', ref: 'a', text: 'New reply' }])
  }
})

test('coalescing adjacent own-chat and delegated turns uses the latest turn outcome', () => {
  const ownWorking = nextTurnStates(null, [bot('a', 'working', false, { completion: null })])
  const ownReply = { id: 'e1', text: 'Earlier own-chat reply' }
  const delegatedWorking = nextTurnStates(ownWorking, [bot('a', 'working', false, { reply: ownReply, completion: null })])
  const delegatedIdle = [bot('a', 'idle', false, { reply: ownReply, completion: null })]
  assert.deepEqual(transitions(delegatedWorking, delegatedIdle), [])
  assert.deepEqual(transitions(ownWorking, delegatedIdle), [])

  const awayWorking = nextTurnStates(null, [bot('a', 'working', false, { away: true, completion: null })])
  const fresh = entry('e2', 'agent', { final: true, text: 'Just completed' })
  const unrelated = entry('e3', 'agent', { final: true, text: 'Later background message' })
  const completed = [bot('a', 'idle', false, { completion: islandReply([fresh, unrelated], undefined, 'e2') })]
  assert.deepEqual(transitions(awayWorking, completed), [{ kind: 'finished', ref: 'a', text: 'Just completed' }])
  assert.deepEqual(transitions(nextTurnStates(awayWorking, completed), completed), [])
  assert.equal(islandReply([unrelated], undefined, 'e2'), null)
})

test('the older-host reply fallback stays quiet across a reconnect', () => {
  const states = nextTurnStates(null, [bot('a', 'working', false, { legacyStreamRev: 1 })])
  const reply = { id: 'e1', text: 'Reply while disconnected' }
  assert.deepEqual(transitions(states, [bot('a', 'idle', false, { reply, legacyStreamRev: 5 })]), [])
})

test('bot-to-bot, group-room, and no-reply turns never pop a stale chat preview (#588)', () => {
  const oldReply = { id: 'e-old', text: 'Old message X' }
  let states = nextTurnStates(null, [bot('a', 'idle', false, { detail: 'Old message X', reply: oldReply })])

  // Bot B sends a message_bot request: Bot A works, replies to Bot B (no new final entry in A's own chat), then goes idle.
  const workingBotReq = [bot('a', 'working', false, { detail: 'Thinking…', reply: oldReply })]
  assert.deepEqual(transitions(states, workingBotReq), [])
  states = nextTurnStates(states, workingBotReq)
  const finishedBotReq = [bot('a', 'idle', false, { detail: 'Old message X', reply: oldReply })]
  assert.deepEqual(transitions(states, finishedBotReq), [], 'delegated turn must not show stale X')
  states = nextTurnStates(states, finishedBotReq)

  // Bot A answers in a group room (away: true), then returns to idle with its old DM reply unchanged.
  const workingGroup = [bot('a', 'working', false, { away: true, detail: 'Thinking…', reply: oldReply })]
  states = nextTurnStates(states, workingGroup)
  const finishedGroup = [bot('a', 'idle', false, { away: false, detail: 'Old message X', reply: oldReply })]
  assert.deepEqual(transitions(states, finishedGroup), [], 'group room turn must not fire DM banner')
  states = nextTurnStates(states, finishedGroup)

  // Mid-turn send_message updates reply while still working: baseline stays pre-turn so turn-end shows the fresh reply.
  const midTurn = [bot('a', 'working', false, { detail: 'Working…', reply: { id: 'e-new', text: 'Fresh Y' } })]
  states = nextTurnStates(states, midTurn)
  const afterMidTurn = [bot('a', 'idle', false, { detail: 'Fresh Y', reply: { id: 'e-new', text: 'Fresh Y' } })]
  assert.deepEqual(transitions(states, afterMidTurn), [{ kind: 'finished', ref: 'a', text: 'Fresh Y' }])
  states = nextTurnStates(states, afterMidTurn)

  // A failure without a new error notice must not recycle an older error notice or stale lastMessage.
  const oldErr = { error: null, errorId: 'err-old', reply: { id: 'e-new', text: 'Fresh Y' } }
  states = nextTurnStates(null, [bot('a', 'idle', false, oldErr)])
  states = nextTurnStates(states, [bot('a', 'working', false, oldErr)])
  const failedStale = [bot('a', 'error', false, { ...oldErr, error: 'Yesterday error', detail: 'Fresh Y' })]
  assert.deepEqual(transitions(states, failedStale), [{ kind: 'failed', ref: 'a', text: null }])
})

test('islandTurnSteps scopes tool calls to the active conversation and current turn', () => {
  const dmTools = [
    entry('t-old', 'tool', { title: 'Old DM tool', toolKind: 'read' }, 100),
    entry('t-cur', 'tool', { title: 'Current DM tool', toolKind: 'execute' }, 250),
  ]
  const groupTools = [
    entry('g-other', 'tool', { title: 'Other bot tool', toolKind: 'read', author: 'b' }, 260),
    entry('g-own', 'tool', { title: 'Group tool by A', toolKind: 'search', author: 'a' }, 270),
  ]
  assert.deepEqual(
    islandTurnSteps({ id: 'a', status: 'working', workingChat: 'a', startedAt: 200 }, dmTools, dmTools, 3, 'Use a tool'),
    [{ title: 'Current DM tool', kind: 'execute' }],
  )
  assert.deepEqual(
    islandTurnSteps({ id: 'a', status: 'working', workingChat: 'room-1', startedAt: 200 }, dmTools, groupTools, 3, 'Use a tool'),
    [{ title: 'Group tool by A', kind: 'search' }],
  )
  assert.deepEqual(
    islandTurnSteps({ id: 'a', status: 'working', workingChat: 'a', startedAt: 300 }, dmTools, dmTools, 3, 'Use a tool'),
    [],
  )
})

test('islandReply and islandLastError ignore non-final, call, and removed entries', () => {
  const entries = [
    entry('u1', 'user', { text: 'Hi' }, 10),
    entry('a1', 'agent', { text: 'Old reply', final: true }, 20),
    entry('a2', 'agent', { text: 'Streaming…', final: false }, 30),
    entry('a3', 'agent', { text: 'Call reply', final: true, call: 'c1' }, 40),
    entry('a4', 'agent', { text: 'Removed', final: true, removed: true }, 50),
    entry('n1', 'notice', { text: 'Oops', style: 'error' }, 60),
  ]
  assert.deepEqual(islandReply(entries), { id: 'a1', text: 'Old reply' })
  assert.deepEqual(islandLastError(entries), { id: 'n1', text: 'Oops' })
  const fileReply = [...entries, entry('a5', 'agent', { final: true, files: [{ id: 'f1', name: 'report.pdf', size: 1, sha256: 'abc', playsInChat: false }] }, 70)]
  assert.deepEqual(islandReply(fileReply), { id: 'a5', text: 'report.pdf' })
})

test('a bot whose chat is idle while an item works counts as working, and its item ending is not a done', () => {
  const background = bot('a', 'idle', true, { reply: { id: 'e2', text: 'Done' } })
  assert.equal(isActive(background), true)
  assert.equal(islandMode(summary([background]), state()), 'compact')
  assert.equal(islandMode(summary([background]), state({ open: true })), 'overview')
  assert.equal(focusOf([bot('b', 'idle'), background])?.ref, 'a')
  // Chat idle + 1 working item → chat idle + no items: the report's own turn is what ends in a done.
  assert.deepEqual(transitions(new Map([['a', turn('idle')]]), [bot('a', 'idle')]), [])
  assert.equal(islandMode(summary([bot('a', 'idle')]), state()), 'hidden')
  // The chat's turn ending with a fresh reply is a done as before, whether or not an item still works.
  assert.deepEqual(
    transitions(new Map([['a', turn('working', { baselineReplyId: 'e1' })]]), [background]),
    [{ kind: 'finished', ref: 'a', text: 'Done' }],
  )
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

test('leaving the message tab while dictating stops it there; other tab changes leave it be', () => {
  assert.equal(stopsDictation('message', 'usage', true), true)
  assert.equal(stopsDictation('message', 'auto', true), true)
  assert.equal(stopsDictation('message', 'message', true), false)
  assert.equal(stopsDictation('message', 'bots', false), false)
  assert.equal(stopsDictation('usage', 'bots', true), false)
})
