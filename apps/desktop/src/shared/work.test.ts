import assert from 'node:assert/strict'
import { test } from 'node:test'
import { draftOf, normalizeBot, type Entry } from './models.ts'
import { allWork, canCutIn, canRetry, capsuleLabel, gitState, givesWay, isThreadMessage, taskLine, normalizeWork, outsideStatus, worksOutside, pendingAsks, pendingCards, readOnlyReason, rosterLine, sharedLine, summaryLine, threadEntries, waitText, whereLine, wherePath, workCounts, workSections, workStatusWord, type WorkTask, type WorkStatus } from './work.ts'

const same = (text: string) => text
const task = (id: string, status: WorkStatus = 'working', change: Partial<WorkTask> = {}): WorkTask => ({
  id, title: `Title ${id}`, status, activity: '', thread: null, wait: null, cutIn: false, createdAt: 1000, startedAt: null, endedAt: null, result: '', source: null, worktree: null, folder: null, talk: false, talkFiles: false, seat: null, ...change,
})
const ids = (tasks: WorkTask[]) => tasks.map((t) => t.id)

test('a bot from a host before the box has no work and no ceiling, and one with a box keeps the tasks it can read', () => {
  const old = normalizeBot({ id: 'b' })
  assert.deepEqual(old.work, [])
  assert.equal(old.maxThreads, null)
  assert.equal('maxThreads' in JSON.parse(JSON.stringify(draftOf(old))), false, 'a save leaves the ceiling out for a host without one')
  const raw = [
    { id: 'i1', title: 'Audit', status: 'needsYou', activity: 'Reading', thread: 'thread-1', wait: null, cutIn: true, createdAt: 3, startedAt: 5, endedAt: null, result: '', source: null, worktree: null },
    { id: 'i2', status: 'waiting', wait: { kind: 'behind', ahead: 2 } },
    { id: 'i3', status: 'waiting', wait: { kind: 'someday' } },
    { id: 'i4', status: 'waiting', wait: { kind: 'behind' } },
    { id: 'i5', status: 'paused' },
    { id: 'i6' },
    { title: 'no id', status: 'working' },
    null,
  ]
  const work = normalizeBot({ id: 'b', work: raw as WorkTask[] }).work
  assert.deepEqual(work, [
    { id: 'i1', title: 'Audit', status: 'needsYou', activity: 'Reading', thread: 'thread-1', wait: null, cutIn: true, createdAt: 3, startedAt: 5, endedAt: null, result: '', source: null, worktree: null, folder: null, talk: false, talkFiles: false, seat: null },
    { id: 'i2', title: '', status: 'waiting', activity: '', thread: null, wait: { kind: 'behind', ahead: 2 }, cutIn: false, createdAt: 0, startedAt: null, endedAt: null, result: '', source: null, worktree: null, folder: null, talk: false, talkFiles: false, seat: null },
    { id: 'i3', title: '', status: 'waiting', activity: '', thread: null, wait: null, cutIn: false, createdAt: 0, startedAt: null, endedAt: null, result: '', source: null, worktree: null, folder: null, talk: false, talkFiles: false, seat: null },
    { id: 'i4', title: '', status: 'waiting', activity: '', thread: null, wait: { kind: 'behind', ahead: 1 }, cutIn: false, createdAt: 0, startedAt: null, endedAt: null, result: '', source: null, worktree: null, folder: null, talk: false, talkFiles: false, seat: null },
  ], 'a status this app does not know, or none, leaves the task out; a reason it does not know reads as none')
})

test('the most threads at once is a whole number from 1 to 6', () => {
  assert.equal(normalizeBot({ id: 'b', maxThreads: 3 }).maxThreads, 3)
  assert.equal(normalizeBot({ id: 'b', maxThreads: 0 }).maxThreads, 1)
  assert.equal(normalizeBot({ id: 'b', maxThreads: 9 }).maxThreads, 6)
  assert.equal(normalizeBot({ id: 'b', maxThreads: '3' as unknown as number }).maxThreads, null)
  assert.equal(draftOf(normalizeBot({ id: 'b', maxThreads: 4 })).maxThreads, 4, 'the settings form edits it')
})

test('the capsule shows only while a task is live or waiting', () => {
  assert.equal(capsuleLabel([], same), null)
  assert.equal(capsuleLabel([task('a', 'done'), task('b', 'failed'), task('c', 'stopped')], same), null)
})

test('one working task and nothing waiting reads as what it does, with its time and Stop', () => {
  const one = task('a', 'working', { activity: 'Running the tests', startedAt: 2000 })
  assert.deepEqual(capsuleLabel([one, task('z', 'done')], same), { mark: 'working', text: 'Running the tests', since: 2000, stops: one })
  assert.equal(capsuleLabel([task('a')], same)!.text, 'Title a', 'its title until it says what it does')
  const hant = (text: string) => (text === 'Thinking…' ? '思考中…' : `[${text}]`)
  assert.equal(capsuleLabel([task('a', 'working', { activity: 'Thinking…' })], hant)!.text, '思考中…', "a step the host names reads in the app's language")
  assert.equal(capsuleLabel([task('a')], hant)!.text, 'Title a', 'its title stays as the bot wrote it')
})

test('several tasks read as how many work and wait', () => {
  const label = (statuses: WorkStatus[]) => capsuleLabel(statuses.map((s, n) => task(String(n), s)), same)!
  assert.deepEqual(label(['working', 'working', 'waiting', 'waiting', 'waiting']), { mark: 'working', text: '2 working · 3 waiting', since: null, stops: null })
  assert.equal(label(['working', 'working']).text, '2 working')
  const oneOfMany = label(['working', 'waiting'])
  assert.equal(oneOfMany.text, '1 working · 1 waiting', 'one working with others waiting counts them')
  assert.equal(oneOfMany.since, null)
  assert.equal(oneOfMany.stops?.id, '0', 'the one working task is the one Stop stops')
  assert.deepEqual(label(['waiting', 'waiting']), { mark: 'waiting', text: '2 waiting', since: null, stops: null })
  assert.equal(label(['waiting', 'done']).text, '1 waiting', 'ended tasks are not counted')
})

test('a task that needs the user leads the capsule, with the counts after it', () => {
  const label = (statuses: WorkStatus[]) => capsuleLabel(statuses.map((s, n) => task(String(n), s)), same)!
  assert.deepEqual(label(['needsYou', 'working', 'working', 'waiting']), { mark: 'needsYou', text: 'Needs you · 2 working · 1 waiting', since: null, stops: null })
  assert.equal(label(['needsYou', 'waiting']).text, 'Needs you · 1 waiting')
  const only = label(['needsYou'])
  assert.equal(only.text, 'Needs you')
  assert.equal(only.stops?.id, '0')
  assert.equal(label(['needsYou', 'needsYou']).text, 'Needs you')
  const hant = (text: string) => ({ 'Needs you': '需要你', '{count} working': '{count} 進行中', '{count} waiting': '{count} 等待中' })[text] ?? text
  assert.equal(capsuleLabel([task('a', 'needsYou'), task('b'), task('c', 'waiting')], hant)!.text, '需要你 · 1 進行中 · 1 等待中', 'the catalog places the count')
})

test('the counts leave out a part at none', () => {
  assert.equal(workCounts([], same), '')
  assert.equal(workCounts([task('a', 'needsYou'), task('b', 'done')], same), '')
  assert.equal(workCounts([task('a'), task('b', 'waiting'), task('c', 'waiting')], same), '1 working · 2 waiting')
})

test('a roster row says the most pressing thing first', () => {
  const work = [task('a', 'working'), task('b', 'working'), task('c', 'waiting')]
  const bot = (status: string, tasks: WorkTask[] = work, activity = '') => ({ status, activity, work: tasks })
  // Needs you: the chat's own card, then a task's.
  assert.deepEqual(rosterLine(bot('needsInput', work, 'Wants to run npm test'), same), { kind: 'needsYou', text: 'Wants to run npm test' })
  assert.deepEqual(rosterLine(bot('needsInput'), same), { kind: 'needsYou', text: 'Needs approval' })
  assert.deepEqual(rosterLine(bot('working', [...work, task('d', 'needsYou')], 'Thinking…'), same), { kind: 'needsYou', text: 'Needs you' })
  // Then what the chat itself does.
  assert.deepEqual(rosterLine(bot('working', work, 'Thinking…'), same), { kind: 'working', text: 'Thinking…' })
  assert.deepEqual(rosterLine(bot('working'), same), { kind: 'working', text: 'Working…' })
  // Then how many work, then how many wait.
  assert.deepEqual(rosterLine(bot('idle'), same), { kind: 'work', text: '2 working · 1 waiting' })
  assert.deepEqual(rosterLine(bot('idle', [task('c', 'waiting')]), same), { kind: 'work', text: '1 waiting' })
  assert.deepEqual(rosterLine(bot('error'), same), { kind: 'work', text: '2 working · 1 waiting' })
  // Else the row reads as usual: its last message.
  assert.equal(rosterLine(bot('idle', []), same), null)
  assert.equal(rosterLine(bot('idle', [task('a', 'done'), task('b', 'stopped')]), same), null)
})

test("the tray menu and the notch island say a bot's line in the roster row's order", () => {
  const work = [task('a', 'working'), task('b', 'working'), task('c', 'waiting')]
  const bot = (status: string, tasks: WorkTask[], activity = '', lastMessage: string | null = 'All done.') => ({ status, activity, work: tasks, lastMessage })
  assert.equal(summaryLine(bot('needsInput', work, 'Wants to run npm test'), same), 'Needs your response')
  assert.equal(summaryLine(bot('working', [...work, task('d', 'needsYou')], 'Thinking…'), same), 'Needs you', "a task's card comes before what the chat does")
  assert.equal(summaryLine(bot('working', work, 'Thinking…'), same), 'Thinking…')
  assert.equal(summaryLine(bot('idle', work), same), '2 working · 1 waiting', 'its work comes before the last message')
  assert.equal(summaryLine(bot('idle', [task('c', 'waiting')]), same), '1 waiting')
  assert.equal(summaryLine(bot('idle', [task('a', 'done')]), same), 'All done.')
  assert.equal(summaryLine(bot('idle', [], '', null), same), null, 'the surface says where the bot works')
  const hant = (text: string) => ({ '{count} working': '{count} 進行中', '{count} waiting': '{count} 等待中' })[text] ?? text
  assert.equal(summaryLine(bot('idle', work), hant), '2 進行中 · 1 等待中')
})

test('a waiting task says why it waits', () => {
  const waiting = (change: Partial<WorkTask>) => task('a', 'waiting', change)
  assert.equal(waitText(waiting({ wait: { kind: 'next', ahead: 0 } }), same), 'Next up')
  assert.equal(waitText(waiting({ wait: { kind: 'behind', ahead: 3 } }), same), '3 ahead')
  assert.equal(waitText(waiting({ wait: { kind: 'after', ahead: 0 } }), same), 'Follows what it works on')
  assert.equal(waitText(waiting({ wait: { kind: 'usage', ahead: 0 } }), same), 'Near the usage limit: one at a time')
  assert.equal(waitText(waiting({ wait: { kind: 'next', ahead: 0 }, cutIn: true }), same), 'Was cut in on; starts again')
  assert.equal(waitText(waiting({}), same), 'Waiting', 'no reason from the host')
  assert.equal(waitText(waiting({ wait: { kind: 'behind', ahead: 2 } }), (text) => (text === '{count} ahead' ? '前面還有 {count} 件' : text)), '前面還有 2 件')
})

test("a task's second line is why it waits, what its thread does, or how it ended", () => {
  assert.equal(taskLine(task('a', 'waiting', { wait: { kind: 'next', ahead: 0 } }), same), 'Next up')
  assert.equal(taskLine(task('a', 'working', { activity: 'Reading' }), same), 'Reading')
  assert.equal(taskLine(task('a', 'working'), same), 'Working')
  assert.equal(taskLine(task('a', 'needsYou', { activity: 'Reading' }), same), 'Needs you')
  assert.equal(taskLine(task('a', 'done', { result: 'All green.' }), same), 'Done')
  assert.equal(taskLine(task('a', 'failed', { result: 'The agent is signed out.' }), same), 'The agent is signed out.')
  assert.equal(taskLine(task('a', 'failed'), same), 'Failed')
  assert.equal(taskLine(task('a', 'stopped'), same), 'Stopped')
  assert.deepEqual((['waiting', 'working', 'needsYou', 'done', 'failed', 'stopped'] as const).map(workStatusWord), ['Waiting', 'Working', 'Needs you', 'Done', 'Failed', 'Stopped'])
})

test('the timeline groups tasks into Threads, the Event box and Recent, each in its order', () => {
  // As the host sends them: live and waiting in dispatch order, then the ended ones, newest first.
  const work = [task('w1', 'working'), task('n1', 'needsYou'), task('w2', 'working'), task('q1', 'waiting'), task('q2', 'waiting'), task('s', 'stopped'), task('d', 'done'), task('f', 'failed')]
  const sections = workSections(work, 4)
  assert.deepEqual(ids(sections.threads), ['n1', 'w1', 'w2'], 'what needs the user comes first')
  assert.deepEqual([sections.atOnce, sections.free], [4, 1], '3 / 4: one thread is free')
  assert.deepEqual(ids(sections.box), ['q1', 'q2'], 'in the order they will be taken')
  assert.deepEqual(ids(sections.recent), ['s', 'd', 'f'])
  const lowered = workSections(work, 1)
  assert.deepEqual([lowered.threads.length, lowered.atOnce, lowered.free], [3, 1, 0], '3 / 1 right after the ceiling was lowered, and none free')
  assert.deepEqual(workSections([], 2), { threads: [], inUse: 0, atOnce: 2, free: 2, box: [], recent: [] })
  assert.deepEqual(workSections([], null), { threads: [], inUse: 0, atOnce: 3, free: 3, box: [], recent: [] }, 'a host that names no ceiling counts 3')
})

test('a thread waiting on the user gives its seat up, as the host says', () => {
  const work = [task('w1', 'working', { seat: true }), task('n1', 'needsYou', { seat: false }), task('q1', 'waiting', { seat: false })]
  const sections = workSections(work, 2)
  assert.deepEqual([sections.threads.length, sections.inUse, sections.free], [2, 1, 1], '1 / 2: the one asking the user holds no seat')
  assert.equal(canCutIn(work, 2), false, 'a seat is free, so nothing has to give way')
  assert.equal(canCutIn(work, 1), true)
  const old = [task('w1', 'working'), task('n1', 'needsYou'), task('q1', 'waiting')]
  assert.deepEqual([workSections(old, 2).inUse, canCutIn(old, 2)], [2, true], 'an older host says nothing: every live task takes one')
  assert.equal(normalizeWork([{ id: 'a', status: 'needsYou', seat: false }])[0]!.seat, false)
})

test('the first in line can cut in only when every thread is busy', () => {
  const work = [task('a', 'working'), task('b', 'needsYou'), task('c', 'waiting')]
  assert.equal(canCutIn(work, 2), true)
  assert.equal(canCutIn(work, 1), true, 'the ceiling was lowered under what runs')
  assert.equal(canCutIn(work, 3), false)
  assert.equal(canCutIn([task('c', 'waiting')], 1), false, 'nothing works, so nothing gives way')
  assert.equal(canCutIn([task('a'), task('b')], 2), false, 'nothing waits')
  assert.equal(canCutIn(work, null), false, 'a host that names no ceiling counts 3')
  assert.equal(canCutIn([task('x'), ...work], null), true)
  for (const kind of ['after', 'usage'] as const) {
    const held = task('c', 'waiting', { wait: { kind, ahead: 1 } })
    assert.equal(canCutIn([task('a'), held], 1), false, `${kind}: a freed thread wouldn't start it`)
  }
})

test('the view of all bots lists the ones with live or waiting tasks, in roster order, and counts what is worked on', () => {
  const roster = [
    { ref: 'idle', bot: { work: [] } },
    { ref: 'ended', bot: { work: [task('a', 'done'), task('b', 'stopped'), task('c', 'failed')] } },
    { ref: 'busy', bot: { work: [task('d', 'working'), task('e', 'working'), task('f', 'waiting'), task('g', 'done')] } },
    { ref: 'asks', bot: { work: [task('h', 'needsYou')] } },
    { ref: 'queued', bot: { work: [task('i', 'waiting')] } },
    { ref: 'one', bot: { work: [task('j', 'working')] } },
  ]
  const all = allWork(roster)
  assert.deepEqual(all.bots.map((r) => r.ref), ['busy', 'asks', 'queued', 'one'])
  assert.equal(all.working, 3, 'tasks that need the user or wait are not counted as working')
  assert.deepEqual(allWork(roster.slice(0, 2)), { bots: [], working: 0 }, 'nothing live or waiting: no button, and an open view closes')
  assert.equal(allWork([roster[3]!, roster[4]!]).working, 0, 'the button shows without a count')
})

test("outside its chat a bot stands as the host says, and as its chat does on an older host", () => {
  const stands = (status: string, outside: { status: string; startedAt: number | null } | null = null) => outsideStatus({ status, outside })
  // The host's word wins, whatever the chat looks like here.
  assert.equal(stands('idle', { status: 'working', startedAt: 5 }), 'working', 'a report the chat has not taken yet')
  assert.equal(stands('working', { status: 'needsInput', startedAt: 5 }), 'needsInput')
  assert.equal(stands('working', { status: 'idle', startedAt: null }), 'idle')
  // A host before `outside`: the chat's own status.
  assert.equal(stands('needsInput'), 'needsInput')
  assert.equal(stands('working'), 'working')
  assert.equal(stands('error'), 'error')
  assert.equal(stands('idle'), 'idle')
  // The tray's working mark.
  assert.equal(worksOutside({ status: 'idle', outside: { status: 'working', startedAt: 1 } }), true)
  assert.equal(worksOutside({ status: 'idle', outside: { status: 'needsInput', startedAt: 1 } }), true)
  assert.equal(worksOutside({ status: 'working', outside: { status: 'idle', startedAt: null } }), false)
  assert.equal(worksOutside({ status: 'working', outside: null }), true)
  assert.equal(worksOutside({ status: 'error', outside: null }), false)
})

test('a bot says how it stands outside its chat, whether its threads get worktrees, and a task its worktree', () => {
  const old = normalizeBot({ id: 'b' })
  assert.deepEqual([old.outside, old.threadWorktrees, old.gitRepo], [null, null, false])
  assert.equal('threadWorktrees' in JSON.parse(JSON.stringify(draftOf(old))), false, 'a save leaves the setting out for a host without it')
  const bot = normalizeBot({ id: 'b', outside: { status: 'working', startedAt: 7 }, threadWorktrees: false, gitRepo: true })
  assert.deepEqual(bot.outside, { status: 'working', startedAt: 7 })
  assert.deepEqual([bot.threadWorktrees, bot.gitRepo, draftOf(bot).threadWorktrees], [false, true, false])
  assert.equal(normalizeBot({ id: 'b', outside: {} as never }).outside, null)
  const [own, none] = normalizeWork([
    { id: 'a', status: 'working', worktree: { branch: 'codync/fix-login', path: '/home/u/.codync/worktrees/b/a' } },
    { id: 'c', status: 'working' },
  ])
  assert.deepEqual(own!.worktree, { branch: 'codync/fix-login', path: '/home/u/.codync/worktrees/b/a' })
  assert.equal(none!.worktree, null)
})

test('a task says where it came from when the bot did not add it', () => {
  const sources = normalizeWork([
    { id: 'a', status: 'working' },
    { id: 'b', status: 'working', source: { kind: 'routine', id: 'r1' } },
    { id: 'c', status: 'working', source: { kind: 'request', id: 'bot-2' } },
    { id: 'd', status: 'working', source: { kind: 'request' } },
    { id: 'e', status: 'working', source: { kind: 'webhook', id: 'x' } },
  ]).map((t) => t.source)
  assert.deepEqual(sources, [null, { kind: 'routine', id: 'r1' }, { kind: 'request', id: 'bot-2' }, { kind: 'request', id: null }, null])
})

test("try again is not offered on another bot's request, and a routine's run or a request never gives way", () => {
  const routine = { kind: 'routine', id: 'r1' } as const
  const request = { kind: 'request', id: 'bot-2' } as const
  // Try again: failed or stopped, the bot's own or a routine's.
  assert.equal(canRetry(task('a', 'failed')), true)
  assert.equal(canRetry(task('a', 'stopped')), true)
  assert.equal(canRetry(task('a', 'stopped', { source: routine })), true, 'it runs the routine again')
  assert.equal(canRetry(task('a', 'failed', { source: request })), false)
  assert.equal(canRetry(task('a', 'stopped', { source: { kind: 'request', id: null } })), false)
  for (const status of ['waiting', 'working', 'needsYou', 'done'] as const) assert.equal(canRetry(task('a', status)), false, status)
  // Giving way: live, and the bot's own.
  assert.equal(givesWay(task('a', 'working')), true)
  assert.equal(givesWay(task('a', 'needsYou')), true)
  assert.equal(givesWay(task('a', 'waiting')), false)
  assert.equal(givesWay(task('a', 'working', { source: routine })), false)
  assert.equal(givesWay(task('a', 'needsYou', { source: request })), false)
  // Cut in is offered only when some live task may give way.
  const waiting = task('w', 'waiting', { wait: { kind: 'next', ahead: 0 } })
  assert.equal(canCutIn([task('a', 'working', { source: routine }), task('b', 'working', { source: request }), waiting], 2), false)
  assert.equal(canCutIn([task('a', 'working', { source: routine }), task('b', 'working'), waiting], 2), true)
  assert.equal(canCutIn([task('a', 'working'), { ...waiting, source: routine }], 1), true, 'a routine task may cut in itself')
})

test('a task that needs the user is asked what its card in the chat asks', () => {
  const card = (id: string, data: Entry['data']): Entry => ({ id, seq: 1, botId: 'b', rev: 1, kind: 'permission', turn: 1, data, createdAt: 0, updatedAt: 0 })
  const chat = [
    card('e1', { status: 'pending', title: 'npm test', task: 'i1', thread: { id: 'thread-1', title: 'Run the suite' } }),
    card('e2', { status: 'allowed', title: 'rm -rf build', task: 'i2' }),
    card('e3', { status: 'pending', title: 'the chat\'s own card' }),
    { ...card('e5', { status: 'pending', decision: true, question: 'Which branch?', task: 'i5' }), kind: 'notice' },
    { ...card('e6', { status: 'answered', decision: true, question: 'Done already', task: 'i6' }), kind: 'notice' },
    { ...card('e4', { status: 'pending', title: 'not a card', task: 'i4' }), kind: 'notice' },
    { ...card('e7', { task: 'i7', connectionRequest: { kind: 'agent', status: 'pending', title: 'Kimi Code' } }), kind: 'notice' },
    { ...card('e8', { task: 'i8', connectionRequest: { kind: 'agent', status: 'ready', title: 'Codex' } }), kind: 'notice' },
    // The turn left Kimi Code signed out, and its fallback was too: Kimi Code's card holds the task's message.
    { ...card('e9', { task: 'i9', connectionRequest: { kind: 'agent', status: 'pending', title: 'Kimi Code', entryIds: ['m9'] } }), kind: 'notice' },
    { ...card('e10', { task: 'i9', connectionRequest: { kind: 'agent', status: 'pending', title: 'Codex', entryIds: [] } }), kind: 'notice' },
    // A sign-in that holds the task's message, then an approval its running turn now waits on.
    { ...card('e11', { task: 'i11', connectionRequest: { kind: 'agent', status: 'pending', title: 'Kimi Code', entryIds: ['m11'] } }), kind: 'notice' },
    card('e12', { status: 'pending', title: 'npm run build', task: 'i11' }),
  ]
  const asks = pendingAsks(chat, (text) => (text === 'Needs {agent} sign-in' ? '需要登入 {agent}' : text))
  assert.deepEqual(
    [...asks],
    [['i1', 'npm test'], ['i5', 'Which branch?'], ['i7', '需要登入 Kimi Code'], ['i9', '需要登入 Kimi Code'], ['i11', 'npm run build']],
    'an approval\'s title, a choice\'s question, the agent a sign-in waits on (the one whose card sends the work again)',
  )
  // The row opens the card it reads.
  assert.equal(pendingCards(chat).get('i9')?.id, 'e9')
  assert.equal(pendingCards(chat).get('i11')?.id, 'e12')
})

test('a task that needs the user opens at its newest waiting card', () => {
  const card = (id: string, status: string): Entry => ({ id, seq: 1, botId: 'b', rev: 1, kind: 'permission', turn: 1, data: { status, task: 'i1' }, createdAt: 0, updatedAt: 0 })
  const entries = [card('e1', 'pending'), card('e2', 'pending'), card('e3', 'allowed')]
  assert.equal(pendingCards(entries).get('i1')?.id, 'e2', 'an answered card is not the one waiting')
  assert.equal(pendingCards(entries).get('i2'), undefined)
})

test('the host says which threads take a message, and a read-only one says whose it is', () => {
  const [own, old] = normalizeWork([{ id: 'a', status: 'working', talk: true, talkFiles: true }, { id: 'b', status: 'working' }])
  assert.equal(own!.talk, true)
  assert.equal(own!.talkFiles, true)
  assert.equal(old!.talk, false, 'a host from before threads names none')
  assert.equal(old!.talkFiles, false, 'nor files')
  assert.equal(readOnlyReason(task('a', 'working', { talk: true })), null)
  assert.equal(readOnlyReason(task('a', 'working', { source: { kind: 'routine', id: 'r1' } })), "Read only: a routine's run")
  assert.equal(readOnlyReason(task('a', 'done', { source: { kind: 'request', id: null } })), "Read only: another bot's request")
  assert.equal(readOnlyReason(task('a', 'done')), null, "the bot's own task that takes none has nothing to explain")
})

test('a task says where its thread works: its branch in a worktree, else its folder with home as ~', () => {
  const worktree = { branch: 'codync/fix-login', path: '/Users/u/.codync/worktrees/b/a' }
  assert.equal(whereLine(task('a'), '/Users/u'), null, 'no thread has taken it')
  assert.equal(normalizeWork([{ id: 'a', status: 'working', folder: '' }])[0]!.folder, null)
  assert.deepEqual(whereLine(task('a', 'working', { folder: '/Users/u/code/app' }), '/Users/u'), { kind: 'folder', text: '~/code/app' })
  assert.deepEqual(whereLine(task('a', 'working', { folder: '/Users/u' }), '/Users/u/'), { kind: 'folder', text: '~' })
  assert.deepEqual(whereLine(task('a', 'working', { folder: '/Users/uma/app' }), '/Users/u'), { kind: 'folder', text: '/Users/uma/app' }, 'another folder that only starts the same')
  assert.deepEqual(whereLine(task('a', 'working', { folder: '/srv/app' }), '/Users/u'), { kind: 'folder', text: '/srv/app' })
  assert.deepEqual(whereLine(task('a', 'working', { folder: '/srv/app' }), ''), { kind: 'folder', text: '/srv/app' }, 'a computer that names no home')
  assert.deepEqual(whereLine(task('a', 'working', { folder: 'C:\\Users\\u\\code\\app' }), 'C:\\Users\\u'), { kind: 'folder', text: '~\\code\\app' })
  assert.deepEqual(whereLine(task('a', 'working', { folder: `${worktree.path}/web`, worktree }), '/Users/u'), { kind: 'branch', text: 'codync/fix-login' })
  // What the line copies is the thread's own folder, which is inside the worktree.
  assert.equal(wherePath(task('a', 'working', { folder: `${worktree.path}/web`, worktree })), `${worktree.path}/web`)
  assert.equal(wherePath(task('a', 'working', { worktree })), worktree.path, 'a host that names no folder')
  assert.equal(wherePath(task('a')), null)
})

test("Settings says whether the bot's folder is a git repository, or that git is missing", () => {
  assert.equal(gitState(normalizeBot({ id: 'b', gitRepo: true })), 'repo', 'a host that does not say counts as having git')
  assert.equal(gitState(normalizeBot({ id: 'b', gitRepo: false, gitInstalled: true })), 'none')
  assert.equal(gitState(normalizeBot({ id: 'b', gitRepo: false, gitInstalled: false })), 'missing')
  assert.equal(gitState({ gitRepo: true, gitInstalled: false }), 'missing')
})

test("a thread's transcript folds what the host told the thread first, and leaves out what the user took back", () => {
  const entry = (id: string, kind: Entry['kind'], data: Entry['data']): Entry => ({ id, seq: 1, botId: 'thread-1', rev: 1, kind, turn: 1, data, createdAt: 0, updatedAt: 0 })
  const brief = entry('e1', 'user', { text: 'Audit\n\nHOLD', from: 'host' })
  const again = entry('e5', 'user', { text: 'A new brief', from: 'host' })
  const typed = entry('e3', 'user', { text: 'also the tests', status: 'queued' })
  const { told, rest } = threadEntries([brief, entry('e2', 'tool', { title: 'ls' }), typed, entry('e4', 'user', { text: 'never mind', status: 'cancelled', removed: true }), again])
  assert.equal(told, brief)
  assert.deepEqual(rest.map((e) => e.id), ['e2', 'e3', 'e5'], 'what it is told later stays in its place')
  assert.deepEqual(threadEntries([typed]), { told: null, rest: [typed] })
  assert.deepEqual(threadEntries([]), { told: null, rest: [] })
})

test("a thread shows what it put in the chat where it came, and a thread's final replies as messages", () => {
  const entry = (id: string, kind: Entry['kind'], data: Entry['data'], createdAt = 0, botId = 'b'): Entry => ({ id, seq: 1, botId, rev: 1, kind, turn: 1, data, createdAt, updatedAt: 0 })
  const thread = { id: 'thread-1', title: 'Fix login' }
  const own = [entry('t1', 'tool', { title: 'ls' }, 10, 'thread-1'), entry('t2', 'agent', { text: 'Done', final: true }, 30, 'thread-1')]
  const chat = [
    entry('c1', 'agent', { text: 'log.txt', final: true, files: [], thread }, 5),
    entry('c2', 'permission', { status: 'allowed', title: 'npm test', thread }, 20),
    entry('c3', 'permission', { status: 'pending', title: "another thread's", thread: { id: 'thread-2', title: 'Docs' } }, 21),
    entry('c4', 'agent', { text: "the chat's own", final: true }, 22),
    entry('c5', 'notice', { status: 'pending', decision: true, question: 'Which?', thread }, 30),
  ]
  assert.deepEqual(threadEntries(own, chat, 'thread-1').rest.map((e) => e.id), ['c1', 't1', 'c2', 't2', 'c5'], 'answered cards too; the thread first at the same moment')
  assert.deepEqual(threadEntries(own, chat, 'thread-9').rest.map((e) => e.id), ['t1', 't2'])
  assert.equal(isThreadMessage(entry('a', 'agent', { text: 'Report', final: true })), true)
  assert.equal(isThreadMessage(entry('a', 'agent', { text: 'Reading the files' })), false, 'what it says on the way is a step')
  assert.equal(isThreadMessage(entry('a', 'user', { text: 'typed', final: true })), false)
})

test('what a thread put in the chat is one line of its thread, saying how it stands', () => {
  const entry = (kind: Entry['kind'], data: Entry['data']): Entry => ({ id: 'c', seq: 1, botId: 'b', rev: 1, kind, turn: 1, data, createdAt: 0, updatedAt: 0 })
  const line = (kind: Entry['kind'], data: Entry['data']) => sharedLine(entry(kind, data), same)
  const options = [{ optionId: 'ok', name: 'Allow', kind: 'allow_once' }, { optionId: 'no', name: 'Deny', kind: 'reject_once' }, { optionId: 'x', name: 'Skip', kind: 'other' }]
  const approval = (status: string, selected?: string) => line('permission', { status, title: 'npm test', options, selected })
  assert.deepEqual(approval('pending'), { symbol: 'hand.raised', text: 'Waiting for your approval: npm test' })
  assert.deepEqual(approval('answered', 'ok'), { symbol: 'checkmark', text: 'Approved: npm test' })
  assert.deepEqual(approval('answered', 'no'), { symbol: 'xmark', text: 'Denied: npm test' })
  assert.equal(approval('answered', 'x')?.text, 'Answered: Skip')
  assert.deepEqual(approval('expired'), { symbol: 'clock', text: 'Not answered: npm test' })
  const choice = (status: string, selected?: string) => line('notice', { decision: true, status, question: 'Squash?', choices: ['Yes', 'No'], selected })
  assert.equal(choice('pending')?.text, 'Asked: Squash?')
  assert.equal(choice('answered', 'No')?.text, 'Answered: No')
  assert.equal(choice('expired')?.text, 'Not answered: Squash?')
  const signIn = (status: string) => line('notice', { text: 'Needs you to sign in.', connectionRequest: { kind: 'agent', status, title: 'Kimi Code' } })
  assert.deepEqual(signIn('pending'), { symbol: 'hand.raised', text: 'Needs Kimi Code sign-in' })
  assert.deepEqual(signIn('ready'), { symbol: 'checkmark', text: 'Signed in' })
  assert.deepEqual(signIn('cancelled'), { symbol: 'xmark', text: 'Cancelled' })
  assert.equal(line('notice', { text: 'Connect', connectionRequest: { kind: 'oauth', status: 'pending', title: 'Linear' } }), null, "a connector's card is not a thread's")
  const file = { id: 'f', name: 'log.txt', size: 3, sha256: '' }
  assert.deepEqual(line('agent', { final: true, files: [file] }), { symbol: 'doc', text: 'Sent a file: log.txt' })
  assert.deepEqual(line('agent', { final: true, files: [file], voiceNote: true, text: 'Hi' }), { symbol: 'waveform', text: 'Sent a voice note' })
  assert.deepEqual(line('agent', { final: true, artifact: { id: 'a', title: 'Plan', updatedAt: 0 } }), { symbol: 'doc.richtext', text: 'Made an artifact: Plan' })
  assert.equal(line('agent', { text: 'Just words', final: true }), null)
  assert.equal(line('notice', { text: 'A notice' }), null)
})
