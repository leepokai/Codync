import { test, expect, type Locator, type Page } from '@playwright/test'
import { readFileSync, unlinkSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, sandbox, workAgent, workBot, typeMessage } from './sandbox.ts'

/** The prompts of the thread working task `title`, as the scripted agent logged them in the bot's folder. */
const threadPrompts = (botId: string, title: string) =>
  readFileSync(join(sandbox().home, 'bots', botId, 'workspace', 'prompts.jsonl'), 'utf8')
    .split('\n').filter(Boolean)
    .map((line) => JSON.parse(line) as { session: string; text: string })
    .filter((p) => p.text.includes('[Task]') && p.text.includes(`"${title}"`))

// The event box (docs/plans/event-box.md) on the scripted agent host/tests/fixtures/work_agent.py:
//   - A message line `ADD <title>|<brief>` in the bot's chat adds a task (reply `Added: {"taskId":…}`); titles are
//     unique. Any message that is no command is answered `reply: <its last line>`.
//   - The brief decides what the task's thread does. `HOLD:<name>`: it works until it is stopped (or a file
//     `release-<name>` appears in the bot's workspace). `PERMISSION`: it asks for approval (a card titled
//     `Run the tests`, one option, shown as `Allow once`) and goes on once answered. Neither: it finishes at once.
//   - `SIGNIN`: the thread's turn fails once asking to sign in, and goes through when its message comes again.
//   - A thread's report comes back to the chat as `Relayed: …`.
//   - What the user writes to a task's thread is a turn of that thread, answered `Thread <their words>: same session`
//     (or `picked up`); words that hold `HOLD:<name>` keep that turn working, and `ASK` raises a choice card
//     (`Which?`, A or B) first.
const HOLD = 'HOLD:e2e'
const ASKS = 'Run the tests'

interface Task { id: string; title: string; status: string }
interface BotState { status: string; maxThreads: number; work: Task[] }

/** The bot as the host has it: an update that changes nothing answers with the bot. */
const botState = async (page: Page, botId: string) => (await api<{ bot: BotState }>(page, 'updateBot', { id: botId })).bot

/** The statuses of the bot's tasks, by title. */
const statuses = async (page: Page, botId: string) => Object.fromEntries((await botState(page, botId)).work.map((t) => [t.title, t.status]))

/** A bot on the scripted agent with its chat open; `maxThreads` sets how many of its threads may run at once. */
async function openBot(page: Page, name: string, maxThreads?: number, backend?: string) {
  const bot = await workBot(page, name, backend)
  if (maxThreads) await api(page, 'updateBot', { id: bot.id, maxThreads })
  await page.locator('.roster-row', { hasText: name }).click()
  return bot.id
}

/** Has the bot add a task, and waits until its box has it and its own turn is over (the next message is a new turn). */
async function addWork(page: Page, botId: string, title: string, brief: string) {
  await api(page, 'send', { botId, text: `ADD ${title}|${brief}` })
  await expect.poll(async () => {
    const bot = await botState(page, botId)
    return bot.status === 'idle' && bot.work.some((t) => t.title === title)
  }).toBe(true)
}

/** Stops the bot's work and deletes it. */
async function cleanUp(page: Page, botId: string | undefined) {
  if (!botId) return
  await api(page, 'stopWork', { botId }).catch(() => {})
  await api(page, 'stop', { botId }).catch(() => {})
  await api(page, 'deleteBot', { botId }).catch(() => {})
}

/** Ends agent `backend`'s rest on this computer, through a bot of its own that switches back to it. */
async function endRest(page: Page, backend: string) {
  const bot = await workBot(page, `Rest ${backend} e2e ${Date.now()}`, backend)
  try {
    await api(page, 'switchBack', { botId: bot.id })
    await expect.poll(async () => ((await api<{ bot: { resting?: unknown[] } }>(page, 'updateBot', { id: bot.id })).bot.resting ?? []).length).toBe(0)
  } finally {
    await api(page, 'deleteBot', { botId: bot.id }).catch(() => {})
  }
}

const capsule = (page: Page) => page.locator('.work-capsule')

/** Opens the bot's Details from the capsule. */
async function openDetails(page: Page) {
  await capsule(page).locator('.work-capsule-open').click()
  const details = page.locator('.details-panel')
  await expect(details.getByRole('tab', { name: 'Details', exact: true })).toHaveAttribute('aria-selected', 'true')
  return details
}

/** A section of the Details timeline, by its title. */
const section = (details: Locator, title: string) =>
  details.locator('.details-section', { has: details.page().locator('.details-section-title', { hasText: new RegExp(`^${title}$`) }) })

/** A task's row in a section. */
const row = (details: Locator, title: string, task: string) => section(details, title).locator('.work-row[data-work-status]', { hasText: task })

/** The thread the inspector shows in place of its pages. */
const threadPanel = (details: Locator) => details.locator('.work-thread')

/** Opens the thread of a task's row; it takes the place of Details, with no dialog. */
async function openThread(details: Locator, taskRow: Locator) {
  await taskRow.locator('.work-row-open').click()
  const thread = threadPanel(details)
  await expect(thread).toBeVisible()
  await expect(details.locator('.details-page.away')).toHaveCount(1)
  await expect(details.page().getByRole('dialog')).toHaveCount(0)
  return thread
}

/** Back to Details from a thread. */
async function closeThread(details: Locator) {
  await threadPanel(details).getByRole('button', { name: 'Back to details' }).click()
  await expect(threadPanel(details)).toHaveCount(0)
  await expect(details.locator('.details-page.away')).toHaveCount(0)
}

/** Writes to the open thread. */
async function typeInThread(details: Locator, text: string) {
  const box = threadPanel(details).getByRole('textbox', { name: 'Message this thread' })
  await box.fill(text)
  await box.press('Enter')
}

/** A row's icon action, which shows once the row is hovered. */
async function hoverAction(taskRow: Locator, name: string) {
  await taskRow.hover()
  await taskRow.getByRole('button', { name, exact: true }).click()
}

test('a working task shows in the capsule and on Details, opens its thread in the panel, and stops once asked', async () => {
  const { browser, page } = await attach()
  const name = `Work e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Audit the repo', HOLD)

    // One working task and nothing waiting: what it does (or its title), and the time counting beside it.
    await expect(capsule(page)).toContainText('Work ·')
    await expect(capsule(page)).toContainText(/\d+:\d\d/)
    await expect(capsule(page)).not.toContainText('1 working')
    // As wide as the composer under it.
    const [above, below] = await Promise.all([capsule(page).boundingBox(), page.locator('.composer-surface').boundingBox()])
    expect(Math.abs(above!.x - below!.x)).toBeLessThanOrEqual(1)
    expect(Math.abs(above!.width - below!.width)).toBeLessThanOrEqual(1)
    // The chat itself is free: the roster row counts its work.
    await expect(page.locator('.roster-row', { hasText: name })).toContainText('1 working')
    // And it answers: a message typed under the capsule gets its reply while the task is worked on.
    await typeMessage(page, 'are you there?')
    await expect(page.locator('.transcript')).toContainText('reply: are you there?')
    await expect(capsule(page)).toBeVisible()

    // The capsule opens Details: what the bot does, with how it is set up on Settings.
    const details = await openDetails(page)
    const working = row(details, 'Threads', 'Audit the repo')
    await expect(working).toHaveAttribute('data-work-status', 'working')
    await expect(working).toContainText(/\d+:\d\d/)
    // One thread of the three allowed runs; the other two show as free. Nothing waits, so there is no box.
    await expect(section(details, 'Threads')).toContainText('1 / 3')
    await expect(section(details, 'Threads').locator('.work-row.free')).toHaveText(['Free', 'Free'])
    await expect(section(details, 'Event box')).toHaveCount(0)
    await expect(section(details, 'Recent')).toHaveCount(0)
    await expect(details.locator('.agent-picker')).toHaveCount(0)

    // A working row opens its thread in the panel, not in a dialog: its title, status and time on top, what it was
    // told folded under one line, and a box to write to it. The chat stays beside it.
    const thread = await openThread(details, working)
    const header = thread.locator('.work-thread-header')
    await expect(header).toContainText('Audit the repo')
    await expect(header).toContainText('Working')
    await expect(header).toContainText(/\d+:\d\d/)
    await expect(header.getByRole('button', { name: 'Stop', exact: true })).toBeVisible()
    const told = thread.locator('.work-thread-told')
    await expect(told).not.toContainText(HOLD)
    await told.getByRole('button', { name: 'What it was told' }).click()
    await expect(told).toContainText(HOLD)
    await expect(thread.getByRole('textbox', { name: 'Message this thread' })).toBeVisible()
    await expect(page.locator('.transcript')).toContainText('reply: are you there?')
    await expect(page.locator('.transcript .work-thread-told')).toHaveCount(0)
    // Back returns to Details, where the row still is.
    await closeThread(details)
    await expect(working).toHaveAttribute('data-work-status', 'working')

    // The capsule's Stop asks once.
    await capsule(page).getByRole('button', { name: 'Stop' }).click()
    const ask = page.getByRole('alertdialog')
    await expect(ask).toContainText('Stop this task?')
    await expect(ask).toContainText('What it already changed stays.')
    await ask.getByRole('button', { name: 'Cancel' }).click()
    await expect(capsule(page)).toBeVisible()
    await capsule(page).getByRole('button', { name: 'Stop' }).click()
    await ask.getByRole('button', { name: 'Stop' }).click()
    await expect(capsule(page)).toHaveCount(0)
    await expect(row(details, 'Recent', 'Audit the repo')).toHaveAttribute('data-work-status', 'stopped')
    await expect(section(details, 'Threads')).toHaveCount(0)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('a message to a working thread waits with Send now and Remove, and one to a done task reopens it', async () => {
  const { browser, page } = await attach()
  const name = `Work thread e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Audit the repo', HOLD)
    await addWork(page, botId, 'Quick look', 'nothing to wait for')
    await expect.poll(async () => (await statuses(page, botId!))['Quick look']).toBe('done')

    // The thread's turn runs, so what is written to it waits: the chat's bubble, with Send now and Remove.
    const details = await openDetails(page)
    const thread = await openThread(details, row(details, 'Threads', 'Audit the repo'))
    await typeInThread(details, 'also check the tests')
    const waiting = thread.locator('.work-thread-mine', { hasText: 'also check the tests' })
    await expect(waiting).toHaveAttribute('data-message-status', 'queued')
    await expect(waiting.getByRole('button', { name: 'Send now', exact: true })).toBeVisible()
    await waiting.getByRole('button', { name: 'Remove', exact: true }).click()
    await expect(waiting).toHaveCount(0)
    // Taken back, it changed nothing: the task works on, and the bot's chat never had the message.
    expect((await statuses(page, botId))['Audit the repo']).toBe('working')
    await expect(page.locator('.transcript')).not.toContainText('also check the tests')
    await closeThread(details)

    // A done task's thread takes a message too: the thread answers it, in the same session.
    const done = row(details, 'Recent', 'Quick look')
    await expect(done).toHaveAttribute('data-work-status', 'done')
    await openThread(details, done)
    await expect(thread.locator('.work-thread-header')).toContainText('Quick look')
    await typeInThread(details, 'look again')
    await expect(thread.locator('.work-thread-mine', { hasText: 'look again' })).toBeVisible()
    await expect(thread.locator('.work-thread-scroll')).toContainText(/Thread look again: (same session|picked up)/)
    // While the turn a message started runs, the task is a thread again: working, back under Threads.
    await expect.poll(async () => (await statuses(page, botId!))['Quick look']).toBe('done')
    await typeInThread(details, 'once more HOLD:again')
    await expect(thread.locator('.work-thread-header')).toContainText('Working')
    await closeThread(details)
    await expect(row(details, 'Threads', 'Quick look')).toHaveAttribute('data-work-status', 'working')
    await expect(row(details, 'Recent', 'Quick look')).toHaveCount(0)
    await expect(section(details, 'Threads')).toContainText('2 / 3')
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("what the thread puts in the chat is one line of its open thread that shows it there, and a routine's run is read only", async () => {
  const { browser, page } = await attach()
  const name = `Work thread card e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Quick look', 'nothing to wait for')
    await expect.poll(async () => (await statuses(page, botId!))['Quick look']).toBe('done')
    await page.getByRole('button', { name: `${name}, view details` }).click()
    const details = page.locator('.details-panel')

    // What the user writes may need them again: the thread's card stays in the chat, and the thread says so in one
    // line, which brings the card into view there.
    const thread = await openThread(details, row(details, 'Recent', 'Quick look'))
    await typeInThread(details, `which one? ASK ${HOLD}`)
    const asked = thread.locator('.work-thread-shared', { hasText: 'Asked: Which?' })
    await expect(asked).toBeVisible()
    await expect(thread.locator('.decision-card')).toHaveCount(0)
    await expect(thread.locator('.work-thread-header')).toContainText('Needs you')
    await asked.click()
    const card = page.locator('.transcript .decision-card', { hasText: 'Which?' })
    await expect(card).toBeInViewport()
    await card.locator('.decision-option', { hasText: 'A' }).click()
    await expect(thread.locator('.work-thread-shared', { hasText: 'Answered: A' })).toBeVisible()
    await expect(thread.locator('.work-thread-header')).toContainText('Working')
    await closeThread(details)

    // A routine's run can be read and stopped, not written to.
    const { routine } = await api<{ routine: { id: string } }>(page, 'saveRoutine', { botId, name: 'Digest', instruction: HOLD, triggers: [{ type: 'webhook' }] })
    await api(page, 'runRoutine', { botId, id: routine.id })
    const run = await openThread(details, row(details, 'Threads', 'Digest'))
    await expect(run.locator('.work-thread-read-only')).toHaveText("Read only: a routine's run")
    await expect(run.getByRole('textbox', { name: 'Message this thread' })).toHaveCount(0)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('with one thread at once the next tasks wait, and First and Drop change the line', async () => {
  const { browser, page } = await attach()
  const name = `Work line e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name, 1)
    await addWork(page, botId, 'First job', HOLD)
    await addWork(page, botId, 'Second job', HOLD)
    await addWork(page, botId, 'Third job', HOLD)

    await expect(capsule(page)).toContainText(/Work ·\s*1 working · 2 waiting/)
    await expect(page.locator('.roster-row', { hasText: name })).toContainText('1 working · 2 waiting')

    const details = await openDetails(page)
    await expect(row(details, 'Threads', 'First job')).toHaveAttribute('data-work-status', 'working')
    await expect(section(details, 'Threads')).toContainText('1 / 1')
    await expect(section(details, 'Threads').locator('.work-row.free')).toHaveCount(0)
    // The box counts what waits and numbers it in the order it will be taken.
    await expect(section(details, 'Event box').locator('.work-section-count')).toHaveText('2')
    const waiting = section(details, 'Event box').locator('.work-row')
    await expect(waiting).toHaveText([/^1Second job.*Next up/, /^2Third job.*1 ahead/])
    // The first in line has no First; each row has Drop.
    await expect(waiting.nth(0).getByRole('button', { name: 'First', exact: true })).toHaveCount(0)

    await hoverAction(waiting.nth(1), 'First')
    await expect(waiting).toHaveText([/^1Third job.*Next up/, /^2Second job.*1 ahead/])
    await expect(row(details, 'Threads', 'First job')).toHaveAttribute('data-work-status', 'working')

    await hoverAction(row(details, 'Event box', 'Second job'), 'Drop')
    await expect(waiting).toHaveText([/Third job/])
    await expect(capsule(page)).toContainText('1 working · 1 waiting')
    expect(await statuses(page, botId)).toEqual({ 'First job': 'working', 'Third job': 'waiting' })
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('the first in line cuts in once the working task to give way is picked', async () => {
  const { browser, page } = await attach()
  const name = `Work cut-in e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name, 1)
    await addWork(page, botId, 'Long job', HOLD)
    await addWork(page, botId, 'Urgent job', HOLD)

    const details = await openDetails(page)
    const urgent = row(details, 'Event box', 'Urgent job')
    await urgent.getByRole('button', { name: 'Cut in' }).click()
    const ask = page.getByRole('alertdialog')
    await expect(ask).toContainText('Let "Urgent job" cut in')
    await expect(ask).toContainText('Every thread is busy. Pick the task that gives way: its thread closes and the task starts again later.')
    await expect(ask.locator('.cut-in-pick')).toHaveText([/Long job/])
    await ask.getByRole('button', { name: 'Cancel' }).click()
    expect(await statuses(page, botId)).toEqual({ 'Long job': 'working', 'Urgent job': 'waiting' })

    await urgent.getByRole('button', { name: 'Cut in' }).click()
    await ask.locator('.cut-in-pick', { hasText: 'Long job' }).click()
    await expect(ask).toHaveCount(0)
    await expect(row(details, 'Threads', 'Urgent job')).toHaveAttribute('data-work-status', 'working')
    // What gave way is first in line again, and says why it waits.
    await expect(row(details, 'Event box', 'Long job')).toContainText('Was cut in on; starts again')
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("a task's approval card in the chat says it comes from the bot's work, and its row leads to it", async () => {
  const { browser, page } = await attach()
  const name = `Work card e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Run the suite', 'PERMISSION first')

    const card = page.locator('.permission-card')
    await expect(card).toContainText('Work · Run the suite')
    await expect(capsule(page)).toContainText(/Work ·\s*Needs you/)
    await expect(page.locator('.roster-row', { hasText: name })).toContainText('Needs you')

    // On Details it leads Threads with what the card asks, and opens the chat at the card.
    const details = await openDetails(page)
    const asking = row(details, 'Threads', 'Run the suite')
    await expect(asking).toHaveAttribute('data-work-status', 'needsYou')
    await expect(asking).toContainText(ASKS)
    await asking.locator('.work-row-open').click()
    await expect(card).toBeInViewport()

    // Answered like any card; the task then finishes, and its report comes back as the bot's message.
    await card.getByRole('button', { name: 'Allow once' }).click()
    await expect(capsule(page)).toHaveCount(0)
    await expect(page.locator('.transcript')).toContainText('Relayed:')
    const done = row(details, 'Recent', 'Run the suite')
    await expect(done).toHaveAttribute('data-work-status', 'done')
    await expect(done).toContainText('Done')
    await expect(done.getByRole('button', { name: 'Try again' })).toHaveCount(0)
    // An ended task is dropped like a waiting one.
    await hoverAction(done, 'Drop')
    await expect(section(details, 'Recent')).toHaveCount(0)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("a thread's agent sign-in is asked in the chat, its row leads to the card, and the task goes on once signed in", async () => {
  const { browser, page } = await attach()
  const name = `Work sign-in e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // As Kimi Code, an agent the host knows by name, so the sign-in its thread needs gets a card.
    botId = await openBot(page, name, undefined, 'kimi')
    await addWork(page, botId, 'Log in', 'SIGNIN')

    const card = page.locator('.request-card', { hasText: 'Work · Log in' })
    await expect(card).toContainText('Kimi Code')
    await expect(capsule(page)).toContainText(/Work ·\s*Needs you/)

    // On Details its row says which agent to sign in to, and opens the chat at the card.
    const details = await openDetails(page)
    const asking = row(details, 'Threads', 'Log in')
    await expect(asking).toHaveAttribute('data-work-status', 'needsYou')
    await expect(asking).toContainText('Needs Kimi Code sign-in')
    await asking.locator('.work-row-open').click()
    await expect(card).toBeInViewport()

    // A turn of the agent that ends well says it's signed in: the task's message goes out again, to the same thread.
    await api(page, 'send', { botId, text: 'hello' })
    await expect(card).toContainText('Signed in')
    await expect(page.locator('.transcript')).toContainText('Relayed:')
    await expect(row(details, 'Recent', 'Log in')).toHaveAttribute('data-work-status', 'done')
    // The new agent process (the old one kept the old sign-in) went on in the thread's session.
    const tries = threadPrompts(botId, 'Log in')
    expect(tries).toHaveLength(2)
    expect(tries[1].session).toBe(tries[0].session)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('when the fallback is signed out too, the row names and opens the sign-in that sends the work again', async () => {
  const { browser, page } = await attach()
  const name = `Work two sign-ins e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // Kimi Code with a Qwen Code fallback (no other spec uses it: it rests signed out after this), both the scripted
    // agent, and every thread turn asks to sign in. Neither rests from an earlier run (a rest is the computer's, for an
    // hour): Switch back ends a bot's own agent's.
    await endRest(page, 'qwen')
    await endRest(page, 'kimi')
    botId = await openBot(page, name, undefined, 'kimi')
    await api(page, 'updateBot', { id: botId, fallbacks: [{ backend: 'qwen', command: fixtureCommand(workAgent), model: null, modelOptions: {} }] })
    const signedOut = join(sandbox().home, 'bots', botId, 'workspace', 'thread-signedout')
    writeFileSync(signedOut, '')
    await addWork(page, botId, 'Two sign-ins', 'anything')

    // Two sign-in cards: Kimi Code's holds the task's message, Qwen Code's is newer.
    const cards = page.locator('.request-card', { hasText: 'Work · Two sign-ins' })
    await expect(cards).toHaveCount(2)
    const kimi = cards.filter({ hasText: 'Kimi Code' })
    await expect(cards.filter({ hasText: 'Qwen Code' })).toHaveCount(1)

    // The row names Kimi Code, whose sign-in sends the work again, and opens the chat at its card.
    const details = await openDetails(page)
    const asking = row(details, 'Threads', 'Two sign-ins')
    await expect(asking).toHaveAttribute('data-work-status', 'needsYou')
    await expect(asking).toContainText('Needs Kimi Code sign-in')
    await asking.locator('.work-row-open').click()
    await expect(kimi).toBeInViewport()

    // Signed in to Kimi Code (a turn of it ends well), the task goes on.
    unlinkSync(signedOut)
    await api(page, 'send', { botId, text: 'hello' })
    await expect(kimi).toContainText('Signed in')
    await expect(row(details, 'Recent', 'Two sign-ins')).toHaveAttribute('data-work-status', 'done')
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("a thread's choice card and the file it shares say they come from the bot's work, and only a press answers", async () => {
  const { browser, page } = await attach()
  const name = `Work choice e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    // The fixture's thread asks `Which?` with the choices A and B, then sends a file.
    await addWork(page, botId, 'Pick one', 'ASK then SHARE')

    const card = page.locator('.decision-card', { hasText: 'Which?' })
    await expect(card).toContainText('Work · Pick one')
    // No row for the user's own words: a typed message is for the bot in the chat, not the thread.
    await expect(card.locator('.decision-own')).toHaveCount(0)
    await expect(capsule(page)).toContainText(/Work ·\s*Needs you/)
    await typeMessage(page, 'are you there?')
    await expect(page.locator('.transcript')).toContainText('reply: are you there?')
    await expect(card.locator('.decision-option')).toHaveCount(2)

    // Its row on Details says what the card asks and opens the chat at it.
    const details = await openDetails(page)
    const asking = row(details, 'Threads', 'Pick one')
    await expect(asking).toHaveAttribute('data-work-status', 'needsYou')
    await expect(asking).toContainText('Which?')
    await asking.locator('.work-row-open').click()
    await expect(card).toBeInViewport()

    await card.locator('.decision-option', { hasText: 'A' }).click()
    // The file the thread shares afterwards is the bot's message, marked the same way.
    await expect(page.locator('.message.agent', { has: page.locator('.file-message') }).locator('.thread-line')).toHaveText('Work · Pick one')
    await expect(capsule(page)).toHaveCount(0)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('a working row stops once asked, and a stopped task tries again', async () => {
  const { browser, page } = await attach()
  const name = `Work stop e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Audit the repo', HOLD)

    const details = await openDetails(page)
    await hoverAction(row(details, 'Threads', 'Audit the repo'), 'Stop')
    const ask = page.getByRole('alertdialog')
    await expect(ask).toContainText('Stop this task?')
    await ask.getByRole('button', { name: 'Cancel' }).click()
    await expect(row(details, 'Threads', 'Audit the repo')).toHaveAttribute('data-work-status', 'working')
    await hoverAction(row(details, 'Threads', 'Audit the repo'), 'Stop')
    await ask.getByRole('button', { name: 'Stop' }).click()

    const stopped = row(details, 'Recent', 'Audit the repo')
    await expect(stopped).toHaveAttribute('data-work-status', 'stopped')
    // Nothing is live or waiting: no threads, and no free rows.
    await expect(section(details, 'Threads')).toHaveCount(0)
    await expect(capsule(page)).toHaveCount(0)

    await stopped.getByRole('button', { name: 'Try again' }).click()
    await expect(row(details, 'Threads', 'Audit the repo')).toHaveAttribute('data-work-status', 'working')
    await expect(section(details, 'Recent')).toHaveCount(0)
    await expect(capsule(page)).toBeVisible()
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("one view lists every bot's work, with the same rows and actions, and leads to a bot's Details and threads", async () => {
  const { browser, page } = await attach()
  const name = `Work all e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // Shown only while some bot has live or waiting tasks.
    const button = page.locator('.sidebar-header').getByRole('button', { name: 'Work', exact: true })
    const others = await button.count()
    botId = await openBot(page, name, 1)
    await addWork(page, botId, 'Long job', HOLD)
    await addWork(page, botId, 'Next job', HOLD)
    await expect(button).toBeVisible()
    // The button counts what is worked on, on every bot.
    if (!others) await expect(page.locator('.sidebar-header .all-work-count')).toHaveText('1')

    await button.click()
    const sheet = page.getByRole('dialog').filter({ has: page.locator('.all-work-list') })
    await expect(sheet).toContainText('Work')
    const group = sheet.locator(`[data-all-work-bot="${name}"]`)
    // The bot's threads, then its box: no titles, no free rows, nothing that ended.
    await expect(group.locator('.work-row')).toHaveText([/Long job/, /^1Next job.*Next up/])
    await expect(group.locator('.work-row.free')).toHaveCount(0)
    await expect(sheet.locator('.details-section-title')).toHaveCount(0)

    // The same actions as on the bot's Details.
    await expect(group.getByRole('button', { name: 'Cut in' })).toBeVisible()
    await hoverAction(group.locator('.work-row', { hasText: 'Next job' }), 'Drop')
    await expect(group.locator('.work-row')).toHaveText([/Long job/])
    expect(await statuses(page, botId)).toEqual({ 'Long job': 'working' })

    // The heading opens that bot's Details.
    await group.locator('.all-work-bot').click()
    await expect(sheet).toHaveCount(0)
    const details = page.locator('.details-panel')
    await expect(details.getByRole('tab', { name: 'Details', exact: true })).toHaveAttribute('aria-selected', 'true')
    await expect(row(details, 'Threads', 'Long job')).toHaveAttribute('data-work-status', 'working')

    // A row of a task a thread has opens that bot's chat with the thread in its inspector.
    await button.click()
    await group.locator('.work-row', { hasText: 'Long job' }).locator('.work-row-open').click()
    await expect(sheet).toHaveCount(0)
    await expect(threadPanel(details).locator('.work-thread-header')).toContainText('Long job')
    await closeThread(details)

    // The view closes by itself when the last task ends, and its button goes with it.
    await button.click()
    await expect(sheet).toBeVisible()
    await api(page, 'stopWork', { botId })
    if (!others) {
      await expect(sheet).toHaveCount(0)
      await expect(button).toHaveCount(0)
    } else {
      await expect(group).toHaveCount(0)
    }
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('Settings holds Instructions and Agent first, and Threads at once steps between 1 and 6', async () => {
  const { browser, page } = await attach()
  const name = `Threads at once e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name, 2)
    await page.getByRole('button', { name: `${name}, view details` }).click()
    const details = page.locator('.details-panel')
    await details.getByRole('tab', { name: 'Settings', exact: true }).click()
    await expect(details.locator('.card-section h3')).toHaveText(['Instructions', 'Agent', 'Profile', 'Workspace', 'Activity', 'Connectors', 'Skills'])

    const stepper = details.locator('.value-row', { hasText: 'Threads at once' })
    await expect(stepper.locator('.stepper-value')).toHaveText('2')
    await stepper.getByRole('button', { name: 'Decrease' }).click()
    await expect(stepper.locator('.stepper-value')).toHaveText('1')
    await expect(stepper.getByRole('button', { name: 'Decrease' })).toBeDisabled()
    await expect.poll(async () => (await botState(page, botId!)).maxThreads).toBe(1)
    await stepper.getByRole('button', { name: 'Increase' }).click()
    await expect.poll(async () => (await botState(page, botId!)).maxThreads).toBe(2)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})
