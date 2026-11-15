import assert from 'node:assert/strict'
import { test } from 'node:test'
import { normalizeBot, type Entry } from './models.ts'
import { isTold, mergeTaskEntries, parseTaskEntry, rosterTaskLine, tasksSince, tasksWhat, taskStatusWord, type BotTask } from './tasks.ts'

const same = (text: string) => text
const task = (id: string, change: Partial<BotTask> = {}): BotTask => ({ id, title: `Title ${id}`, status: 'working', activity: '', startedAt: 1000, start: 'fork', ...change })
const entry = (id: string, seq: number, rev: number, text = id): Entry => ({ id, seq, botId: 'task-1', rev, kind: 'agent', turn: 1, data: { text }, createdAt: 0, updatedAt: 0 })

test('a bot from a host before tasks has none, and one with tasks keeps the ones it can read', () => {
  assert.deepEqual(normalizeBot({ id: 'b' }).tasks, [])
  const raw = [{ id: 't1', title: 'Audit', status: 'needsInput', activity: 'Reading', startedAt: 5, start: 'fork' }, { id: 't2' }, { title: 'no id' }, null]
  const tasks = normalizeBot({ id: 'b', tasks: raw as BotTask[] }).tasks
  assert.deepEqual(tasks, [
    { id: 't1', title: 'Audit', status: 'needsInput', activity: 'Reading', startedAt: 5, start: 'fork' },
    { id: 't2', title: '', status: 'working', activity: '', startedAt: 0, start: 'brief' },
  ])
  assert.equal(normalizeBot({ id: 'b', tasks: [{ ...task('t'), status: 'later' as BotTask['status'] }] }).tasks[0]!.status, 'working', 'a status a newer host adds reads as working')
})

test('a taskEntry event names its task and carries one entry', () => {
  const parsed = parseTaskEntry({ type: 'taskEntry', rev: 7, task: 'task-1', entry: { id: 'e1', seq: 1, botId: 'task-1', rev: 7, kind: 'tool', turn: 1 } })
  assert.equal(parsed?.task, 'task-1')
  assert.deepEqual(parsed?.entry.data, {}, 'an entry without data still reads')
  assert.equal(parseTaskEntry({ type: 'taskEntry', entry: { id: 'e1' } }), null, 'no task')
  assert.equal(parseTaskEntry({ type: 'taskEntry', task: 'task-1' }), null, 'no entry')
})

test('one task reads as what it is doing', () => {
  assert.equal(tasksWhat([task('a', { activity: 'Running the tests' })], same), 'Running the tests')
  assert.equal(tasksWhat([task('a')], same), 'Title a', 'its title until it says what it does')
  assert.equal(tasksWhat([task('a', { status: 'starting', activity: 'x' })], same), 'Starting…')
  assert.equal(tasksWhat([task('a', { status: 'needsInput', activity: 'x' })], same), 'Needs you')
  const hant = (text: string) => (text === 'Thinking…' ? '思考中…' : text)
  assert.equal(tasksWhat([task('a', { activity: 'Thinking…' })], hant), '思考中…', 'a step the host names reads in the app\'s language')
  assert.equal(tasksWhat([task('a')], (text) => `[${text}]`), 'Title a', 'its title stays as the bot wrote it')
})

test('several tasks read as how many, whether one needs the user, and the oldest one\'s time', () => {
  const two = [task('a', { startedAt: 3000 }), task('b', { startedAt: 2000 })]
  assert.equal(tasksWhat(two, same), '2 tasks running')
  assert.equal(tasksSince(two), 2000)
  assert.equal(tasksWhat([...two, task('c', { status: 'needsInput' })], same), '3 tasks running · needs you')
  assert.equal(tasksWhat(two, (text) => (text === '{count} tasks running' ? '{count} 件事' : text)), '2 件事')
})

test('a roster row says what goes on in the background only while its chat is idle', () => {
  const tasks = [task('a', { activity: 'Reading' })]
  assert.equal(rosterTaskLine({ status: 'idle', tasks }, same), 'In the background: Reading')
  assert.equal(rosterTaskLine({ status: 'working', tasks }, same), null)
  assert.equal(rosterTaskLine({ status: 'idle', tasks: [] }, same), null)
  const hant = (text: string) => (text === 'In the background: {what}' ? '背景：{what}' : text)
  assert.equal(rosterTaskLine({ status: 'idle', tasks }, hant), '背景：Reading', 'the catalog places the words')
})

test('every status has its word', () => {
  assert.deepEqual((['starting', 'working', 'needsInput', 'done', 'failed', 'stopped'] as const).map(taskStatusWord),
    ['Starting…', 'Working', 'Needs you', 'Done', 'Failed', 'Stopped'])
})

test('a task\'s transcript holds each entry once, the higher rev, oldest first', () => {
  const fetched = [entry('a', 1, 1), entry('b', 2, 2, 'old')]
  const live = mergeTaskEntries(fetched, [entry('c', 3, 5), entry('b', 2, 4, 'new')])
  assert.deepEqual(live.map((e) => [e.id, e.data.text]), [['a', 'a'], ['b', 'new'], ['c', 'c']])
  // A fetch that answers after a live update doesn't undo it.
  assert.equal(mergeTaskEntries(live, [entry('b', 2, 2, 'old')]).find((e) => e.id === 'b')!.data.text, 'new')
  assert.equal(mergeTaskEntries(live, fetched).length, 3)
})

test('what the host told a task is not the user\'s own message', () => {
  assert.equal(isTold({ ...entry('a', 1, 1), kind: 'user', data: { text: 'brief', from: 'host' } }), true)
  assert.equal(isTold({ ...entry('a', 1, 1), kind: 'user', data: { text: 'typed' } }), false)
  assert.equal(isTold(entry('a', 1, 1)), false)
})
