import assert from 'node:assert/strict'
import { test } from 'node:test'
import { draftOf, normalizeBot, type Entry } from './models.ts'
import { canCutIn, capsuleLabel, itemLine, pendingAsks, rosterLine, summaryLine, waitText, workCounts, workSections, workStatusWord, type WorkItem, type WorkStatus } from './work.ts'

const same = (text: string) => text
const item = (id: string, status: WorkStatus = 'working', change: Partial<WorkItem> = {}): WorkItem => ({
  id, title: `Title ${id}`, status, activity: '', task: null, wait: null, cutIn: false, createdAt: 1000, startedAt: null, endedAt: null, result: '', ...change,
})
const ids = (items: WorkItem[]) => items.map((i) => i.id)

test('a bot from a host before the box has no work and no ceiling, and one with a box keeps the items it can read', () => {
  const old = normalizeBot({ id: 'b' })
  assert.deepEqual(old.work, [])
  assert.equal(old.maxAgents, null)
  assert.equal('maxAgents' in JSON.parse(JSON.stringify(draftOf(old))), false, 'a save leaves the ceiling out for a host without one')
  const raw = [
    { id: 'i1', title: 'Audit', status: 'needsYou', activity: 'Reading', task: 'task-1', wait: null, cutIn: true, createdAt: 3, startedAt: 5, endedAt: null, result: '' },
    { id: 'i2', status: 'waiting', wait: { kind: 'behind', ahead: 2 } },
    { id: 'i3', status: 'waiting', wait: { kind: 'someday' } },
    { id: 'i4', status: 'waiting', wait: { kind: 'behind' } },
    { id: 'i5', status: 'paused' },
    { id: 'i6' },
    { title: 'no id', status: 'working' },
    null,
  ]
  const work = normalizeBot({ id: 'b', work: raw as WorkItem[] }).work
  assert.deepEqual(work, [
    { id: 'i1', title: 'Audit', status: 'needsYou', activity: 'Reading', task: 'task-1', wait: null, cutIn: true, createdAt: 3, startedAt: 5, endedAt: null, result: '' },
    { id: 'i2', title: '', status: 'waiting', activity: '', task: null, wait: { kind: 'behind', ahead: 2 }, cutIn: false, createdAt: 0, startedAt: null, endedAt: null, result: '' },
    { id: 'i3', title: '', status: 'waiting', activity: '', task: null, wait: null, cutIn: false, createdAt: 0, startedAt: null, endedAt: null, result: '' },
    { id: 'i4', title: '', status: 'waiting', activity: '', task: null, wait: { kind: 'behind', ahead: 1 }, cutIn: false, createdAt: 0, startedAt: null, endedAt: null, result: '' },
  ], 'a status this app does not know, or none, leaves the item out; a reason it does not know reads as none')
})

test('the most agents at once is a whole number from 1 to 6', () => {
  assert.equal(normalizeBot({ id: 'b', maxAgents: 3 }).maxAgents, 3)
  assert.equal(normalizeBot({ id: 'b', maxAgents: 0 }).maxAgents, 1)
  assert.equal(normalizeBot({ id: 'b', maxAgents: 9 }).maxAgents, 6)
  assert.equal(normalizeBot({ id: 'b', maxAgents: '3' as unknown as number }).maxAgents, null)
  assert.equal(draftOf(normalizeBot({ id: 'b', maxAgents: 4 })).maxAgents, 4, 'the settings form edits it')
})

test('the capsule shows only while an item is live or waiting', () => {
  assert.equal(capsuleLabel([], same), null)
  assert.equal(capsuleLabel([item('a', 'done'), item('b', 'failed'), item('c', 'stopped')], same), null)
})

test('one working item and nothing waiting reads as what it does, with its time and Stop', () => {
  const one = item('a', 'working', { activity: 'Running the tests', startedAt: 2000 })
  assert.deepEqual(capsuleLabel([one, item('z', 'done')], same), { mark: 'working', text: 'Running the tests', since: 2000, stops: one })
  assert.equal(capsuleLabel([item('a')], same)!.text, 'Title a', 'its title until it says what it does')
  const hant = (text: string) => (text === 'Thinking…' ? '思考中…' : `[${text}]`)
  assert.equal(capsuleLabel([item('a', 'working', { activity: 'Thinking…' })], hant)!.text, '思考中…', "a step the host names reads in the app's language")
  assert.equal(capsuleLabel([item('a')], hant)!.text, 'Title a', 'its title stays as the bot wrote it')
})

test('several items read as how many work and wait', () => {
  const label = (statuses: WorkStatus[]) => capsuleLabel(statuses.map((s, n) => item(String(n), s)), same)!
  assert.deepEqual(label(['working', 'working', 'waiting', 'waiting', 'waiting']), { mark: 'working', text: '2 working · 3 waiting', since: null, stops: null })
  assert.equal(label(['working', 'working']).text, '2 working')
  const oneOfMany = label(['working', 'waiting'])
  assert.equal(oneOfMany.text, '1 working · 1 waiting', 'one working with others waiting counts them')
  assert.equal(oneOfMany.since, null)
  assert.equal(oneOfMany.stops?.id, '0', 'the one working item is the one Stop stops')
  assert.deepEqual(label(['waiting', 'waiting']), { mark: 'waiting', text: '2 waiting', since: null, stops: null })
  assert.equal(label(['waiting', 'done']).text, '1 waiting', 'ended items are not counted')
})

test('an item that needs the user leads the capsule, with the counts after it', () => {
  const label = (statuses: WorkStatus[]) => capsuleLabel(statuses.map((s, n) => item(String(n), s)), same)!
  assert.deepEqual(label(['needsYou', 'working', 'working', 'waiting']), { mark: 'needsYou', text: 'Needs you · 2 working · 1 waiting', since: null, stops: null })
  assert.equal(label(['needsYou', 'waiting']).text, 'Needs you · 1 waiting')
  const only = label(['needsYou'])
  assert.equal(only.text, 'Needs you')
  assert.equal(only.stops?.id, '0')
  assert.equal(label(['needsYou', 'needsYou']).text, 'Needs you')
  const hant = (text: string) => ({ 'Needs you': '需要你', '{count} working': '{count} 進行中', '{count} waiting': '{count} 等待中' })[text] ?? text
  assert.equal(capsuleLabel([item('a', 'needsYou'), item('b'), item('c', 'waiting')], hant)!.text, '需要你 · 1 進行中 · 1 等待中', 'the catalog places the count')
})

test('the counts leave out a part at none', () => {
  assert.equal(workCounts([], same), '')
  assert.equal(workCounts([item('a', 'needsYou'), item('b', 'done')], same), '')
  assert.equal(workCounts([item('a'), item('b', 'waiting'), item('c', 'waiting')], same), '1 working · 2 waiting')
})

test('a roster row says the most pressing thing first', () => {
  const work = [item('a', 'working'), item('b', 'working'), item('c', 'waiting')]
  const bot = (status: string, items: WorkItem[] = work, activity = '') => ({ status, activity, work: items })
  // Needs you: the chat's own card, then an item's.
  assert.deepEqual(rosterLine(bot('needsInput', work, 'Wants to run npm test'), same), { kind: 'needsYou', text: 'Wants to run npm test' })
  assert.deepEqual(rosterLine(bot('needsInput'), same), { kind: 'needsYou', text: 'Needs your approval' })
  assert.deepEqual(rosterLine(bot('working', [...work, item('d', 'needsYou')], 'Thinking…'), same), { kind: 'needsYou', text: 'Needs you' })
  // Then what the chat itself does.
  assert.deepEqual(rosterLine(bot('working', work, 'Thinking…'), same), { kind: 'working', text: 'Thinking…' })
  assert.deepEqual(rosterLine(bot('working'), same), { kind: 'working', text: 'Working…' })
  // Then how many work, then how many wait.
  assert.deepEqual(rosterLine(bot('idle'), same), { kind: 'background', text: '2 working · 1 waiting' })
  assert.deepEqual(rosterLine(bot('idle', [item('c', 'waiting')]), same), { kind: 'background', text: '1 waiting' })
  assert.deepEqual(rosterLine(bot('error'), same), { kind: 'background', text: '2 working · 1 waiting' })
  // Else the row reads as usual: its last message.
  assert.equal(rosterLine(bot('idle', []), same), null)
  assert.equal(rosterLine(bot('idle', [item('a', 'done'), item('b', 'stopped')]), same), null)
})

test("the tray menu and the notch island say a bot's line in the roster row's order", () => {
  const work = [item('a', 'working'), item('b', 'working'), item('c', 'waiting')]
  const bot = (status: string, items: WorkItem[], activity = '', lastMessage: string | null = 'All done.') => ({ status, activity, work: items, lastMessage })
  assert.equal(summaryLine(bot('needsInput', work, 'Wants to run npm test'), same), 'Needs your response')
  assert.equal(summaryLine(bot('working', [...work, item('d', 'needsYou')], 'Thinking…'), same), 'Needs you', "an item's card comes before what the chat does")
  assert.equal(summaryLine(bot('working', work, 'Thinking…'), same), 'Thinking…')
  assert.equal(summaryLine(bot('idle', work), same), '2 working · 1 waiting', 'work in the background comes before the last message')
  assert.equal(summaryLine(bot('idle', [item('c', 'waiting')]), same), '1 waiting')
  assert.equal(summaryLine(bot('idle', [item('a', 'done')]), same), 'All done.')
  assert.equal(summaryLine(bot('idle', [], '', null), same), null, 'the surface says where the bot works')
  const hant = (text: string) => ({ '{count} working': '{count} 進行中', '{count} waiting': '{count} 等待中' })[text] ?? text
  assert.equal(summaryLine(bot('idle', work), hant), '2 進行中 · 1 等待中')
})

test('a waiting item says why it waits', () => {
  const waiting = (change: Partial<WorkItem>) => item('a', 'waiting', change)
  assert.equal(waitText(waiting({ wait: { kind: 'next', ahead: 0 } }), same), 'Next up')
  assert.equal(waitText(waiting({ wait: { kind: 'behind', ahead: 3 } }), same), '3 ahead')
  assert.equal(waitText(waiting({ wait: { kind: 'after', ahead: 0 } }), same), 'Follows what it works on')
  assert.equal(waitText(waiting({ wait: { kind: 'usage', ahead: 0 } }), same), 'Waiting for usage')
  assert.equal(waitText(waiting({ wait: { kind: 'next', ahead: 0 }, cutIn: true }), same), 'Was cut in on; starts again')
  assert.equal(waitText(waiting({}), same), 'Waiting', 'no reason from the host')
  assert.equal(waitText(waiting({ wait: { kind: 'behind', ahead: 2 } }), (text) => (text === '{count} ahead' ? '前面還有 {count} 件' : text)), '前面還有 2 件')
})

test("an item's second line is why it waits, what its task does, or how it ended", () => {
  assert.equal(itemLine(item('a', 'waiting', { wait: { kind: 'next', ahead: 0 } }), same), 'Next up')
  assert.equal(itemLine(item('a', 'working', { activity: 'Reading' }), same), 'Reading')
  assert.equal(itemLine(item('a', 'working'), same), 'Working')
  assert.equal(itemLine(item('a', 'needsYou', { activity: 'Reading' }), same), 'Needs you')
  assert.equal(itemLine(item('a', 'done', { result: 'All green.' }), same), 'Done')
  assert.equal(itemLine(item('a', 'failed', { result: 'The agent is signed out.' }), same), 'The agent is signed out.')
  assert.equal(itemLine(item('a', 'failed'), same), 'Failed')
  assert.equal(itemLine(item('a', 'stopped'), same), 'Stopped')
  assert.deepEqual((['waiting', 'working', 'needsYou', 'done', 'failed', 'stopped'] as const).map(workStatusWord), ['Waiting', 'Working', 'Needs you', 'Done', 'Failed', 'Stopped'])
})

test('the timeline groups items into Threads, the Event box and Recent, each in its order', () => {
  // As the host sends them: live and waiting in dispatch order, then the ended ones, newest first.
  const work = [item('w1', 'working'), item('n1', 'needsYou'), item('w2', 'working'), item('q1', 'waiting'), item('q2', 'waiting'), item('s', 'stopped'), item('d', 'done'), item('f', 'failed')]
  const sections = workSections(work, 4)
  assert.deepEqual(ids(sections.threads), ['n1', 'w1', 'w2'], 'what needs the user comes first')
  assert.deepEqual([sections.places, sections.free], [4, 1], '3 / 4: one place is free')
  assert.deepEqual(ids(sections.box), ['q1', 'q2'], 'in the order they will be taken')
  assert.deepEqual(ids(sections.recent), ['s', 'd', 'f'])
  const lowered = workSections(work, 1)
  assert.deepEqual([lowered.threads.length, lowered.places, lowered.free], [3, 1, 0], '3 / 1 right after the ceiling was lowered, and no place free')
  assert.deepEqual(workSections([], 2), { threads: [], places: 2, free: 2, box: [], recent: [] })
  assert.deepEqual(workSections([], null), { threads: [], places: 3, free: 3, box: [], recent: [] }, 'a host that names no ceiling counts 3')
})

test('the first in line can cut in only when every place is taken', () => {
  const work = [item('a', 'working'), item('b', 'needsYou'), item('c', 'waiting')]
  assert.equal(canCutIn(work, 2), true)
  assert.equal(canCutIn(work, 1), true, 'the ceiling was lowered under what runs')
  assert.equal(canCutIn(work, 3), false)
  assert.equal(canCutIn([item('c', 'waiting')], 1), false, 'nothing works, so nothing gives way')
  assert.equal(canCutIn([item('a'), item('b')], 2), false, 'nothing waits')
  assert.equal(canCutIn(work, null), false, 'a host that names no ceiling counts 3')
  assert.equal(canCutIn([item('x'), ...work], null), true)
})

test('an item that needs the user is asked what its card in the chat asks', () => {
  const card = (id: string, data: Entry['data']): Entry => ({ id, seq: 1, botId: 'b', rev: 1, kind: 'permission', turn: 1, data, createdAt: 0, updatedAt: 0 })
  const asks = pendingAsks([
    card('e1', { status: 'pending', title: 'npm test', item: 'i1', task: { id: 'task-1', title: 'Run the suite' } }),
    card('e2', { status: 'allowed', title: 'rm -rf build', item: 'i2' }),
    card('e3', { status: 'pending', title: 'the chat\'s own card' }),
    { ...card('e4', { status: 'pending', title: 'not a card', item: 'i4' }), kind: 'notice' },
  ])
  assert.deepEqual([...asks], [['i1', 'npm test']])
})
