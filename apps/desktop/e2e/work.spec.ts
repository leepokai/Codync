import { test, expect, type Locator, type Page } from '@playwright/test'
import { readFileSync, unlinkSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, openDetails, openPage, sandbox, typeMessage, workAgent, workBot } from './sandbox.ts'

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
//   - `WORKFLOW:<name>` (on an agent started with `tasks`): its turn ends while a workflow it reports runs in the
//     background, which ends once `release-<name>` appears; then the agent reports by itself.
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
async function openDetailsFromCapsule(page: Page) {
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

/** The thread the inspector shows in a tab, in the chat's form. */
const threadPanel = (details: Locator) => details.locator('.thread-chat')

/** The open thread's status, on top of its tab. */
const threadStatus = (details: Locator) => threadPanel(details).locator('.thread-chat-status')

/** Opens the thread of a task's row: a new tab of the pane, named by the task and shown, with no dialog. */
async function openThread(details: Locator, taskRow: Locator) {
  await taskRow.locator('.work-row-open').click()
  const thread = threadPanel(details)
  await expect(thread).toBeVisible()
  // A thread's tab carries its task's name as its tip; the views' tabs have none.
  await expect(details.getByRole('tab', { selected: true })).toHaveAttribute('title', /.+/)
  await expect(details.page().getByRole('dialog')).toHaveCount(0)
  return thread
}

/** Closes the thread's tab (the pane shows the tab beside it), then back to Details. */
async function closeThread(details: Locator) {
  const tab = details.getByRole('tab', { selected: true })
  const name = (await tab.getAttribute('title')) ?? ''
  await details.getByRole('button', { name: `Close ${name}` }).click()
  await expect(threadPanel(details)).toHaveCount(0)
  await expect(details.getByRole('tab', { selected: true })).not.toHaveAttribute('title', /.+/)
  await details.getByRole('tab', { name: 'Details', exact: true }).click()
}

/** Writes to the open thread. */
async function typeInThread(details: Locator, text: string) {
  const box = threadPanel(details).getByRole('textbox', { name: 'Message this thread' })
  await box.fill(text)
  await box.press('Enter')
}

/** A 1×1 PNG, as base64. */
const PIXEL = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII='

/** Drops a picture called `name` on the open thread's box. */
async function dropImageInThread(details: Locator, name: string) {
  await threadPanel(details).locator('.composer').evaluate((box, [file, pixel]) => {
    const data = new DataTransfer()
    data.items.add(new File([Uint8Array.from(atob(pixel!), (c) => c.charCodeAt(0))], file!, { type: 'image/png' }))
    box.dispatchEvent(new DragEvent('drop', { dataTransfer: data, bubbles: true, cancelable: true }))
  }, [name, PIXEL])
  // A picture's chip is its thumbnail, named only by its remove button.
  await expect(threadPanel(details).locator('.file-chips').getByRole('button', { name: `Remove ${name}` })).toBeVisible()
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
    const details = await openDetailsFromCapsule(page)
    const working = row(details, 'Threads', 'Audit the repo')
    await expect(working).toHaveAttribute('data-work-status', 'working')
    await expect(working).toContainText(/\d+:\d\d/)
    // One thread of the three allowed runs; the other two show as free. Nothing waits, so there is no box.
    await expect(section(details, 'Threads')).toContainText('1 / 3')
    await expect(section(details, 'Threads').locator('.work-row.free')).toHaveText(['Free', 'Free'])
    await expect(section(details, 'Event box')).toHaveCount(0)
    await expect(section(details, 'Recent')).toHaveCount(0)
    await expect(details.locator('.model-picker-field')).toHaveCount(0)

    // A working row opens its thread in the panel, in the chat's form, not in a dialog: its tab named by the task, its
    // status and time on top, what it was told folded under one line, and the chat's box to write to it, which stops
    // it while empty and shows the agent it runs on. The chat stays beside it.
    const thread = await openThread(details, working)
    await expect(details.getByRole('tab', { selected: true })).toHaveAttribute('title', 'Audit the repo')
    const header = threadStatus(details)
    await expect(header).toContainText('Working')
    await expect(header).toContainText(/\d+:\d\d/)
    await expect(thread.getByRole('button', { name: 'Stop', exact: true })).toBeVisible()
    await expect(thread.locator('.composer-toolbar .thread-option')).toHaveAttribute('title', 'Agent')
    const told = thread.locator('.work-thread-told')
    await expect(told).not.toContainText(HOLD)
    await told.getByRole('button', { name: 'What it was told' }).click()
    await expect(told).toContainText(HOLD)
    await expect(thread.getByRole('textbox', { name: 'Message this thread' })).toBeVisible()
    const chat = page.locator('.transcript:not(.thread-chat .transcript)')
    await expect(chat).toContainText('reply: are you there?')
    await expect(chat.locator('.work-thread-told')).toHaveCount(0)
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
    const details = await openDetailsFromCapsule(page)
    const thread = await openThread(details, row(details, 'Threads', 'Audit the repo'))
    await typeInThread(details, 'also check the tests')
    const waiting = thread.locator('.work-thread-mine', { hasText: 'also check the tests' })
    await expect(waiting).toHaveAttribute('data-message-status', 'queued')
    await expect(waiting.getByRole('button', { name: 'Send now', exact: true })).toBeVisible()
    await waiting.getByRole('button', { name: 'Remove', exact: true }).click()
    await expect(waiting).toHaveCount(0)
    // Taken back, it changed nothing: the task works on, and the bot's chat never had the message.
    expect((await statuses(page, botId))['Audit the repo']).toBe('working')
    await expect(page.locator('.transcript:not(.thread-chat .transcript)')).not.toContainText('also check the tests')

    // A picture goes with a message, waits with it as a preview, and Remove takes both back.
    await expect(thread.getByRole('button', { name: 'Add files', exact: true })).toBeVisible()
    await dropImageInThread(details, 'shot.png')
    await typeInThread(details, 'see this')
    const withPicture = thread.locator('.work-thread-mine', { hasText: 'see this' })
    await expect(withPicture).toHaveAttribute('data-message-status', 'queued')
    await expect(withPicture.locator('.attachment-image-button img')).toBeVisible()
    await withPicture.getByRole('button', { name: 'Remove', exact: true }).click()
    await expect(withPicture).toHaveCount(0)
    await closeThread(details)

    // A done task's thread takes a message too: the thread answers it, in the same session.
    const done = row(details, 'Recent', 'Quick look')
    await expect(done).toHaveAttribute('data-work-status', 'done')
    await openThread(details, done)
    await expect(details.getByRole('tab', { selected: true })).toHaveAttribute('title', 'Quick look')
    await typeInThread(details, 'look again')
    await expect(thread.locator('.work-thread-mine', { hasText: 'look again' })).toBeVisible()
    await expect(thread).toContainText(/Thread look again: (same session|picked up)/)
    // While the turn a message started runs, the task is a thread again: working, back under Threads.
    await expect.poll(async () => (await statuses(page, botId!))['Quick look']).toBe('done')
    // A picture alone to the done task reopens it too, and shows in its transcript.
    await dropImageInThread(details, 'more.png')
    await threadPanel(details).getByRole('textbox', { name: 'Message this thread' }).press('Enter')
    await expect(thread.locator('.work-thread-mine .attachment-image-button img')).toBeVisible()
    // The thread answers it in a turn of its own (the agent answers each message `Thread …`).
    await expect.poll(async () => (await thread.locator('.chat-row').allInnerTexts()).join('\n').match(/Thread /g)?.length ?? 0).toBe(2)
    await expect.poll(async () => (await statuses(page, botId!))['Quick look']).toBe('done')
    await typeInThread(details, 'once more HOLD:again')
    await expect(threadStatus(details)).toContainText('Working')
    await closeThread(details)
    await expect(row(details, 'Threads', 'Quick look')).toHaveAttribute('data-work-status', 'working')
    await expect(row(details, 'Recent', 'Quick look')).toHaveCount(0)
    await expect(section(details, 'Threads')).toContainText('2 / 3')
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("what the thread puts in the chat is answered in its open thread too, and a routine's run is read only", async () => {
  const { browser, page } = await attach()
  const name = `Work thread card e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Quick look', 'nothing to wait for')
    await expect.poll(async () => (await statuses(page, botId!))['Quick look']).toBe('done')
    const details = await openDetails(page, 'Details')

    // What the user writes may need them again: the thread's card is the chat's own, shown and answered in the thread
    // as in the chat.
    const thread = await openThread(details, row(details, 'Recent', 'Quick look'))
    await typeInThread(details, `which one? ASK ${HOLD}`)
    const card = thread.locator('.decision-card', { hasText: 'Which?' })
    await expect(card).toBeVisible()
    await expect(threadStatus(details)).toContainText('Needs you')
    await card.locator('.decision-option', { hasText: 'A' }).click()
    await expect(threadStatus(details)).toContainText('Working')
    await closeThread(details)

    // A routine's run can be read and stopped, not written to.
    const { routine } = await api<{ routine: { id: string } }>(page, 'saveRoutine', { botId, name: 'Digest', instruction: HOLD, triggers: [{ type: 'webhook' }] })
    await api(page, 'runRoutine', { botId, id: routine.id })
    const run = await openThread(details, row(details, 'Threads', 'Digest'))
    await expect(run.locator('.work-thread-read-only')).toHaveText("Read only: a routine's run")
    await expect(run.getByRole('textbox', { name: 'Message this thread' })).toHaveCount(0)
    await run.locator('.work-thread-read-only').getByRole('button', { name: 'Stop', exact: true }).click()
    await page.getByRole('alertdialog').getByRole('button', { name: 'Stop', exact: true }).click()
    await expect(run.locator('.work-thread-read-only').getByRole('button', { name: 'Stop', exact: true })).toHaveCount(0)
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

    const details = await openDetailsFromCapsule(page)
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

test("a routine's next runs wait for the one under way without keeping a free thread from the bot's other task", async () => {
  const { browser, page } = await attach()
  const name = `Work held runs e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // Both threads busy, so the routine's runs queue up in the box ahead of the bot's other task.
    botId = await openBot(page, name, 2)
    await addWork(page, botId, 'First job', 'HOLD:busy')
    await addWork(page, botId, 'Second job', 'HOLD:busy')
    const { routine } = await api<{ routine: { id: string } }>(page, 'saveRoutine', { botId, name: 'Digest', instruction: HOLD, triggers: [{ type: 'webhook' }] })
    await api(page, 'runRoutine', { botId, id: routine.id })
    await api(page, 'runRoutine', { botId, id: routine.id })
    await expect.poll(async () => (await botState(page, botId!)).work.filter((t) => t.title === 'Digest').length).toBe(2)
    await addWork(page, botId, 'Side job', HOLD)

    // Both threads free up: the first run takes one, the next run waits for it, and the other task takes the second.
    writeFileSync(join(sandbox().home, 'bots', botId, 'workspace', 'release-busy'), '')
    await expect.poll(async () => (await statuses(page, botId!))['Side job']).toBe('working')
    const runs = (await botState(page, botId)).work.filter((t) => t.title === 'Digest').map((t) => t.status)
    expect(runs.sort()).toEqual(['waiting', 'working'])
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("a thread the user started never gives way: the cut-in ask lists only the bot's own working tasks", async () => {
  const { browser, page } = await attach()
  const name = `Work cut-in seat e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name, 1)
    await api(page, 'startThread', { botId, text: `Mine ${HOLD}` })
    await addWork(page, botId, 'Long job', HOLD)
    await addWork(page, botId, 'Urgent job', HOLD)
    await expect.poll(() => statuses(page, botId!)).toEqual({ [`Mine ${HOLD}`]: 'working', 'Long job': 'working', 'Urgent job': 'waiting' })

    const details = await openDetailsFromCapsule(page)
    await row(details, 'Event box', 'Urgent job').getByRole('button', { name: 'Cut in' }).click()
    const ask = page.getByRole('alertdialog')
    await expect(ask.locator('.cut-in-pick')).toHaveText([/Long job/])
    await ask.getByRole('button', { name: 'Cancel' }).click()
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

    const details = await openDetailsFromCapsule(page)
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
    const details = await openDetailsFromCapsule(page)
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
    const details = await openDetailsFromCapsule(page)
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
    const details = await openDetailsFromCapsule(page)
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
    const details = await openDetailsFromCapsule(page)
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

    const details = await openDetailsFromCapsule(page)
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

test("the Threads page groups every bot's work as chosen, shows a thread beside the list and starts one", async () => {
  const { browser, page } = await attach()
  const name = `Work all e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const count = page.locator('.nav-rail').getByRole('button', { name: 'Threads', exact: true }).locator('.nav-rail-count')
    const others = await count.count()
    botId = await openBot(page, name, 1)
    await addWork(page, botId, 'Long job', HOLD)
    await addWork(page, botId, 'Next job', HOLD)
    // The rail counts what is worked on, on every bot.
    if (!others) await expect(count).toHaveText('1')

    await openPage(page, 'Threads')
    const list = page.locator('aside.sidebar')
    // New thread is the header's +; grouped by bot (the list's options), a group per bot.
    await expect(page.locator('.sidebar-header').getByRole('button', { name: 'New thread', exact: true })).toBeVisible()
    await page.locator('.sidebar-header .computer-filter').click()
    const byBot = page.getByRole('menuitemradio', { name: 'Group by bot', exact: true })
    if ((await byBot.getAttribute('aria-checked')) !== 'true') await byBot.click()
    await page.keyboard.press('Escape')
    const section = list.getByRole('group', { name, exact: true })
    const thread = (title: string) => section.locator(`[data-thread-row="${title}"]`)
    await expect(section.locator('[data-thread-row]')).toHaveCount(2)
    await expect(thread('Long job')).toHaveAttribute('data-work-status', 'working')
    await expect(thread('Next job')).toHaveAttribute('data-work-status', 'waiting')
    // Each row starts with the coding agent's icon (a custom command's is the terminal), a waiting one's too.
    for (const title of ['Long job', 'Next job']) await expect(thread(title).locator('.thread-agent > *')).toHaveCount(1)

    // Search in the header finds a thread by its bot's name (one waiting has none yet) and picks it in the list.
    await page.locator('.sidebar-header').getByRole('button', { name: 'Search', exact: true }).click()
    await page.getByRole('dialog').getByRole('textbox', { name: 'Search', exact: true }).fill(name)
    await expect(page.getByRole('dialog').getByRole('option')).toHaveText([/^Long job/])
    await page.keyboard.press('Enter')
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(thread('Long job')).toHaveClass(/selected/)

    // A row of an item a task has shows its thread beside the list, in the chat's form: its title and status in the
    // window's bar, the chat's message box under its messages.
    await thread('Long job').click()
    const header = page.locator('.window-bar')
    const shown = page.locator('.thread-chat')
    await expect(header).toContainText('Long job')
    await expect(header).toContainText('Working')
    await expect(shown.getByRole('textbox')).toHaveAttribute('placeholder', 'Message this thread')

    // The empty box stops what works there. What ends stays in its group, and its thread stays open.
    await shown.getByRole('button', { name: 'Stop', exact: true }).click()
    await page.getByRole('alertdialog').getByRole('button', { name: 'Stop', exact: true }).click()
    await expect(thread('Long job')).toHaveAttribute('data-work-status', 'stopped')
    await expect(header).toContainText('Long job')

    // The `+` on a bot's heading starts a thread with it: its message opens the thread, which shows beside the list.
    await section.locator('.thread-section-toggle').hover()
    await section.getByRole('button', { name: 'New thread', exact: true }).click()
    const start = page.locator('.new-thread')
    await expect(start).toContainText(`New thread with ${name}`)
    // What it runs on starts as the bot's own, each a chip in the box's row; how it is approved is in the model's panel.
    await expect(start.locator('.thread-option')).toHaveCount(3)
    await start.locator('.thread-option[title="Model"]').click()
    const approval = page.locator('.model-picker-permissions')
    await approval.click()
    await page.getByRole('menuitemradio', { name: 'Approve automatically' }).click()
    await expect(approval).toContainText('Approve automatically')
    await page.keyboard.press('Escape')
    await start.getByRole('textbox').fill('Count the files')
    await start.getByRole('textbox').press('Enter')
    // The bot's one place is taken (Next job works), but a thread the user starts takes none: it runs at once (and,
    // told nothing that holds it, is done), picked, its thread beside the list.
    await expect(thread('Count the files')).toHaveClass(/selected/)
    await expect(thread('Count the files')).toHaveAttribute('data-work-status', 'done')
    await expect(header).toContainText('Count the files')
  } finally {
    await openPage(page, 'Bots').catch(() => {})
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('a new thread starts with a picture: it opens with the file, read from the thread', async () => {
  const { browser, page } = await attach()
  const name = `Thread files e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await openPage(page, 'Threads')
    await page.locator('.sidebar-header').getByRole('button', { name: 'New thread', exact: true }).click()
    const start = page.locator('.new-thread')
    const chip = start.locator('.file-chips').getByRole('button', { name: 'Remove shot.png' })
    // With No bot (where it starts) the box's + is there, as in a chat; a dropped picture waits in it as a chip.
    await expect(start.getByRole('button', { name: 'Add files', exact: true })).toBeVisible()
    await start.locator('.composer').evaluate((box, pixel) => {
      const data = new DataTransfer()
      data.items.add(new File([Uint8Array.from(atob(pixel), (c) => c.charCodeAt(0))], 'shot.png', { type: 'image/png' }))
      box.dispatchEvent(new DragEvent('drop', { dataTransfer: data, bubbles: true, cancelable: true }))
    }, PIXEL)
    await expect(chip).toBeVisible()
    // Picking a bot after it keeps the picture.
    await start.locator('.thread-option[title="Choose a bot"]').click()
    await page.getByRole('menuitemradio', { name, exact: true }).click()
    await expect(start).toContainText(`New thread with ${name}`)
    await expect(chip).toBeVisible()
    await start.getByRole('textbox').fill('Look at the picture')
    await start.getByRole('textbox').press('Enter')
    // Sent, the thread opens with the message and its picture.
    const thread = page.locator('.thread-chat')
    await expect(thread).toContainText('Look at the picture')
    await expect(thread.locator('.work-thread-mine .attachment-image-button img')).toBeVisible()
    // Sent, the picture is gone from the next new thread's box.
    await page.locator('.sidebar-header').getByRole('button', { name: 'New thread', exact: true }).click()
    await expect(start.getByRole('button', { name: 'Add files', exact: true })).toBeVisible()
    await expect(chip).toHaveCount(0)
  } finally {
    await openPage(page, 'Bots').catch(() => {})
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("a thread's menu, from its … on hover or a right-click, offers what applies to it and does it", async () => {
  const { browser, page } = await attach()
  const name = `Work menu e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name, 1)
    await addWork(page, botId, 'Long job', HOLD)
    await addWork(page, botId, 'Next job', HOLD)
    await addWork(page, botId, 'Last job', HOLD)
    await openPage(page, 'Threads')
    const list = page.locator('aside.sidebar')
    await page.locator('.sidebar-header .computer-filter').click()
    const byBot = page.getByRole('menuitemradio', { name: 'Group by bot', exact: true })
    if ((await byBot.getAttribute('aria-checked')) !== 'true') await byBot.click()
    await page.keyboard.press('Escape')
    const section = list.getByRole('group', { name, exact: true })
    const item = (title: string) => section.locator('.thread-item', { has: page.locator(`[data-thread-row="${title}"]`) })
    const menu = page.getByRole('menu')
    const titles = () => menu.getByRole('menuitem').allTextContents()

    // The … shows on hover, in the mark's place, and opens the menu: the last in line can go first.
    await item('Last job').hover()
    await item('Last job').getByRole('button', { name: 'More actions for Last job' }).click()
    expect(await titles()).toEqual(['Open chat', 'New thread', 'First', 'Copy title', 'Drop'])
    await menu.getByRole('menuitem', { name: 'First' }).click()
    // Now first in line with every place taken, it can cut in instead.
    await item('Last job').click({ button: 'right' })
    expect(await titles()).toEqual(['Open chat', 'New thread', 'Cut in', 'Copy title', 'Drop'])
    await page.keyboard.press('Escape')

    // A working one stops, asked first; then it can be tried again or dropped.
    await item('Long job').click({ button: 'right' })
    expect(await titles()).toEqual(['Open thread', 'Open chat', 'New thread', 'Stop', 'Copy title', 'Copy folder path'])
    await menu.getByRole('menuitem', { name: 'Stop' }).click()
    await page.getByRole('alertdialog').getByRole('button', { name: 'Stop', exact: true }).click()
    await expect(section.locator('[data-thread-row="Long job"]')).toHaveAttribute('data-work-status', 'stopped')
    await item('Long job').click({ button: 'right' })
    await expect(menu.getByRole('menuitem', { name: 'Try again' })).toBeVisible()
    await menu.getByRole('menuitem', { name: 'Drop' }).click()
    await expect(item('Long job')).toHaveCount(0)

    // Open chat goes to the bot's chat on the Bots page.
    await item('Next job').click({ button: 'right' })
    await menu.getByRole('menuitem', { name: 'Open chat' }).click()
    await expect(page.locator('.roster-row.selected', { hasText: name })).toBeVisible()
  } finally {
    // Open chat may already have gone to Bots, whose entry pressed again would fold the list away.
    if (!(await page.locator('.nav-rail [data-page="bots"][aria-current="page"]').count())) await openPage(page, 'Bots').catch(() => {})
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
    const details = await openDetails(page, 'Settings')
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

test('a thread waiting on the user gives its seat up: the next task starts, and the first goes on once answered', async () => {
  const { browser, page } = await attach()
  const name = `Work seat e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name, 1)
    // The first thread asks for approval once `release-seat` is in the bot's folder; the second waits for its seat.
    await addWork(page, botId, 'Asks first', 'PERMISSION:seat')
    await addWork(page, botId, 'Goes next', HOLD)
    expect(await statuses(page, botId)).toEqual({ 'Asks first': 'working', 'Goes next': 'waiting' })

    // Its card raised, the first thread gives its seat up and the next task starts beside it.
    writeFileSync(join(sandbox().home, 'bots', botId, 'workspace', 'release-seat'), '')
    const card = page.locator('.permission-card', { hasText: 'Work · Asks first' })
    await expect(card).toBeVisible()
    await expect.poll(async () => (await statuses(page, botId!))['Goes next']).toBe('working')
    const details = await openDetailsFromCapsule(page)
    await expect(row(details, 'Threads', 'Asks first')).toHaveAttribute('data-work-status', 'needsYou')
    await expect(row(details, 'Threads', 'Goes next')).toHaveAttribute('data-work-status', 'working')
    // One seat of one in use: the thread waiting on the user takes none, and no Free row or box is left.
    await expect(section(details, 'Threads').locator('.work-section-count')).toHaveText('1 / 1')
    await expect(section(details, 'Threads').locator('.work-row.free')).toHaveCount(0)
    await expect(section(details, 'Event box')).toHaveCount(0)

    // Answered, it goes on beside the other and reports.
    await card.getByRole('button', { name: 'Allow once' }).click()
    await expect(page.locator('.transcript')).toContainText('Report Asks first')
    await expect(row(details, 'Recent', 'Asks first')).toHaveAttribute('data-work-status', 'done')
    await expect(row(details, 'Threads', 'Goes next')).toHaveAttribute('data-work-status', 'working')
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('the bot passes words on to a working task: its thread shows them as told, and it reports once after them', async () => {
  const { browser, page } = await attach()
  const name = `Work tell e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Audit the repo', 'HOLD:tell')

    // The chat's tell_task: the task takes the words once its current turn ends, before it reports.
    await api(page, 'send', { botId, text: 'TELL Audit the repo|and the tests' })
    await expect(page.locator('.transcript')).toContainText('Told:')
    await expect(page.locator('.transcript')).toContainText('before it reports')

    // In its thread they read as what it was told, not as the user's message waiting with Send now.
    const details = await openDetailsFromCapsule(page)
    const thread = await openThread(details, row(details, 'Threads', 'Audit the repo'))
    await expect(thread.locator('.work-thread-quote', { hasText: 'and the tests' })).toBeVisible()
    await expect(thread.locator('.work-thread-mine', { hasText: 'and the tests' })).toHaveCount(0)
    await closeThread(details)

    // Its turn over, the thread takes them, and the task reports once: after them.
    writeFileSync(join(sandbox().home, 'bots', botId, 'workspace', 'release-tell'), '')
    await expect(row(details, 'Recent', 'Audit the repo')).toHaveAttribute('data-work-status', 'done')
    await expect(page.locator('.transcript')).toContainText('Relayed: Heard and the tests')
    await expect(page.locator('.transcript .message.agent', { hasText: 'Relayed:' })).toHaveCount(1)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('a thread that ends its turn while its background work runs is held: it reads the ask as told and reports once', async () => {
  const { browser, page } = await attach()
  const name = `Work held e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Review the PR', 'DETACH:held')

    // Its turn ended with a progress line, but the command it left runs: the task is still working.
    const details = await openDetailsFromCapsule(page)
    const thread = await openThread(details, row(details, 'Threads', 'Review the PR'))
    await expect(thread.locator('.work-thread-quote', { hasText: 'you ended your turn while' })).toBeVisible()
    await expect.poll(async () => (await statuses(page, botId!))['Review the PR']).toBe('working')
    await expect(page.locator('.transcript .message.agent', { hasText: 'Relayed:' })).toHaveCount(0)
    await closeThread(details)

    // Once the command is done, the thread reports once: its real report.
    writeFileSync(join(sandbox().home, 'bots', botId, 'workspace', 'release-held'), '')
    await expect(row(details, 'Recent', 'Review the PR')).toHaveAttribute('data-work-status', 'done')
    await expect(page.locator('.transcript')).toContainText('Relayed: Report Review the PR: it finished, all green')
    await expect(page.locator('.transcript .message.agent', { hasText: 'Relayed:' })).toHaveCount(1)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('a held thread that ends its turn again while its background work runs stopped early: a failed task', async () => {
  const { browser, page } = await attach()
  const name = `Work held again e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Watch the CI', 'DETACH:again AGAIN')

    const details = await openDetailsFromCapsule(page)
    await expect(row(details, 'Recent', 'Watch the CI')).toHaveAttribute('data-work-status', 'failed')
    await expect(page.locator('.transcript').first()).toContainText('still ran')
  } finally {
    if (botId) writeFileSync(join(sandbox().home, 'bots', botId, 'workspace', 'release-again'), '')
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("a thread whose agent runs a workflow in the background shows it over its message box until it ends", async () => {
  const { browser, page } = await attach()
  const name = `Work background e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    // Like Claude's adapter, it reports its background work to a thread (`asyncTasks`).
    await api(page, 'updateBot', { id: botId, command: `${fixtureCommand(workAgent)} tasks` })
    await addWork(page, botId, 'Review the PR', 'WORKFLOW:capsule')

    const details = await openDetailsFromCapsule(page)
    const thread = await openThread(details, row(details, 'Threads', 'Review the PR'))
    const background = thread.locator('[data-background-capsule]')
    await expect(background).toContainText('after-change · Review: diff')
    await expect.poll(async () => (await statuses(page, botId!))['Review the PR']).toBe('working')

    writeFileSync(join(sandbox().home, 'bots', botId, 'workspace', 'release-capsule'), '')
    await expect(background).toHaveCount(0)
    await expect.poll(async () => (await statuses(page, botId!))['Review the PR']).toBe('done')
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('the bot looks back at a finished task: its status, result and thread steps', async () => {
  const { browser, page } = await attach()
  const name = `Work recall e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Quick look', 'nothing to wait for')
    await expect.poll(async () => (await statuses(page, botId!))['Quick look']).toBe('done')

    // The fixture's recall_task: by words for the task, then by its id for the whole of it.
    await typeMessage(page, 'RECALL Quick look')
    const recalled = page.locator('.transcript .message.agent', { hasText: 'Recalled' })
    await expect(recalled).toContainText('Recalled Quick look: done | Report Quick look:')
    await expect(recalled).toContainText(/\| [1-9]\d* steps/)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("a thread the user starts runs at once past the bot's ceiling, stays out of its work_status, and is theirs to write to and stop", async () => {
  const { browser, page } = await attach()
  const name = `Work user thread e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // The bot's one seat is taken by its own task.
    botId = await openBot(page, name, 1)
    await addWork(page, botId, 'Busy job', HOLD)

    // A thread started from the Threads page doesn't wait for that seat: it works at once.
    await openPage(page, 'Threads')
    await page.locator('aside.sidebar .sidebar-header-action[aria-label="New thread"]').click()
    const start = page.locator('.new-thread')
    const chooser = start.locator('.thread-option[title="Choose a bot"]')
    if (await chooser.isEnabled()) {
      await chooser.click()
      await page.getByRole('menuitemradio', { name, exact: true }).click()
    }
    await expect(start).toContainText(`New thread with ${name}`)
    await start.getByRole('textbox').fill(`Mine ${HOLD}`)
    await start.getByRole('textbox').press('Enter')
    await expect.poll(() => statuses(page, botId!)).toEqual({ 'Busy job': 'working', [`Mine ${HOLD}`]: 'working' })

    // The user still writes to it and stops it, from its page.
    const thread = page.locator('main')
    const box = thread.getByRole('textbox', { name: 'Message this thread' })
    await box.fill('also check the tests')
    await box.press('Enter')
    await expect(thread.locator('.work-thread-mine', { hasText: 'also check the tests' })).toBeVisible()

    // The bot's own work_status lists its task, not the user's thread.
    await api(page, 'send', { botId, text: 'STATUS' })
    await openPage(page, 'Bots')
    await page.locator('.roster-row', { hasText: name }).click()
    const chat = page.locator('.transcript:not(.thread-chat .transcript)')
    await expect(chat).toContainText('Status:')
    const said = (await chat.locator('.chat-row', { hasText: 'Status:' }).last().innerText())
    expect(said).toContain('Busy job')
    expect(said).not.toContain('Mine')

    await openPage(page, 'Threads')
    await page.locator('aside.sidebar .thread-row', { hasText: `Mine ${HOLD}` }).first().click()
    await box.fill('')
    await thread.getByRole('button', { name: 'Stop', exact: true }).click()
    const ask = page.getByRole('alertdialog')
    if (await ask.isVisible().catch(() => false)) await ask.getByRole('button', { name: 'Stop' }).click()
    await expect.poll(async () => (await statuses(page, botId!))[`Mine ${HOLD}`]).toBe('stopped')
    expect((await statuses(page, botId!))['Busy job']).toBe('working')
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})
