import { test, expect, type Locator, type Page } from '@playwright/test'
import { api, attach, taskBot, typeMessage } from './sandbox.ts'

// The event box (docs/plans/event-box.md) on the scripted agent host/tests/fixtures/task_agent.py:
//   - A message line `ADD <title>|<brief>` in the bot's chat adds an item (reply `Added: {"itemId":…}`); titles are
//     unique. Any message that is no command is answered `reply: <its last line>`.
//   - The brief decides what the item's task does. `HOLD:<name>`: it works until it is stopped (or a file
//     `release-<name>` appears in the bot's workspace). `PERMISSION`: it asks for approval (a card titled
//     `Run the tests`, one option, shown as `Allow once`) and goes on once answered. Neither: it finishes at once.
//   - A task's report comes back to the chat as `Relayed: …`.
const HOLD = 'HOLD:e2e'
const ASKS = 'Run the tests'

interface Item { id: string; title: string; status: string }
interface BotState { status: string; maxAgents: number; work: Item[] }

/** The bot as the host has it: an update that changes nothing answers with the bot. */
const botState = async (page: Page, botId: string) => (await api<{ bot: BotState }>(page, 'updateBot', { id: botId })).bot

/** The statuses of the bot's items, by title. */
const statuses = async (page: Page, botId: string) => Object.fromEntries((await botState(page, botId)).work.map((i) => [i.title, i.status]))

/** A bot on the scripted agent with its chat open; `maxAgents` sets how many of its agents may run at once. */
async function openBot(page: Page, name: string, maxAgents?: number) {
  const bot = await taskBot(page, name)
  if (maxAgents) await api(page, 'updateBot', { id: bot.id, maxAgents })
  await page.locator('.roster-row', { hasText: name }).click()
  return bot.id
}

/** Has the bot add an item, and waits until its box has it and its own turn is over (the next message is a new turn). */
async function addWork(page: Page, botId: string, title: string, brief: string) {
  await api(page, 'send', { botId, text: `ADD ${title}|${brief}` })
  await expect.poll(async () => {
    const bot = await botState(page, botId)
    return bot.status === 'idle' && bot.work.some((i) => i.title === title)
  }).toBe(true)
}

/** Stops the bot's work and deletes it. */
async function cleanUp(page: Page, botId: string | undefined) {
  if (!botId) return
  await api(page, 'stopWork', { botId }).catch(() => {})
  await api(page, 'stop', { botId }).catch(() => {})
  await api(page, 'deleteBot', { botId }).catch(() => {})
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

/** An item's row in a section. */
const row = (details: Locator, title: string, item: string) => section(details, title).locator('.work-row[data-work-status]', { hasText: item })

/** A row's icon action, which shows once the row is hovered. */
async function hoverAction(itemRow: Locator, name: string) {
  await itemRow.hover()
  await itemRow.getByRole('button', { name, exact: true }).click()
}

test('a working item shows in the capsule and on Details, opens its transcript, and stops once asked', async () => {
  const { browser, page } = await attach()
  const name = `Work e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Audit the repo', HOLD)

    // One working item and nothing waiting: what it does (or its title), and the time counting beside it.
    await expect(capsule(page)).toContainText('In the background ·')
    await expect(capsule(page)).toContainText(/\d+:\d\d/)
    await expect(capsule(page)).not.toContainText('1 working')
    // The chat itself is free: the roster row counts what goes on in the background.
    await expect(page.locator('.roster-row', { hasText: name })).toContainText('1 working')
    // And it answers: a message typed under the capsule gets its reply while the item is worked on.
    await typeMessage(page, 'are you there?')
    await expect(page.locator('.transcript')).toContainText('reply: are you there?')
    await expect(capsule(page)).toBeVisible()

    // The capsule opens Details: what the bot does, with how it is set up on Settings.
    const details = await openDetails(page)
    const working = row(details, 'Threads', 'Audit the repo')
    await expect(working).toHaveAttribute('data-work-status', 'working')
    await expect(working).toContainText(/\d+:\d\d/)
    // One thread of the three allowed runs; the other two places show as free. Nothing waits, so there is no box.
    await expect(section(details, 'Threads')).toContainText('1 / 3')
    await expect(section(details, 'Threads').locator('.work-row.free')).toHaveText(['Free', 'Free'])
    await expect(section(details, 'Event box')).toHaveCount(0)
    await expect(section(details, 'Recent')).toHaveCount(0)
    await expect(details.locator('.agent-picker')).toHaveCount(0)

    // A working row opens its task's transcript: what it was told, as a quote, and nothing to type into.
    await working.locator('.work-row-open').click()
    const transcript = page.getByRole('dialog').filter({ has: page.locator('.trace-content') })
    await expect(transcript).toContainText('Audit the repo')
    await expect(transcript).toContainText('Working')
    await expect(transcript.locator('.task-told').first()).toContainText(HOLD)
    await expect(transcript.locator('textarea')).toHaveCount(0)
    await transcript.getByRole('button', { name: 'Close' }).click()
    await expect(transcript).toHaveCount(0)
    // None of the task's transcript is in the chat.
    await expect(page.locator('.transcript .task-told')).toHaveCount(0)

    // The capsule's Stop asks once.
    await capsule(page).getByRole('button', { name: 'Stop' }).click()
    const ask = page.getByRole('alertdialog')
    await expect(ask).toContainText('Stop the background work?')
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

test('with one agent at once the next items wait, and First and Drop change the line', async () => {
  const { browser, page } = await attach()
  const name = `Work line e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name, 1)
    await addWork(page, botId, 'First job', HOLD)
    await addWork(page, botId, 'Second job', HOLD)
    await addWork(page, botId, 'Third job', HOLD)

    await expect(capsule(page)).toContainText(/In the background ·\s*1 working · 2 waiting/)
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

test('the first in line cuts in once the working item to give way is picked', async () => {
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
    await expect(ask).toContainText('Every place is taken. Pick what gives way: its session closes and it starts again later.')
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

test("an item's approval card in the chat says it comes from the background, and its row leads to it", async () => {
  const { browser, page } = await attach()
  const name = `Work card e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Run the suite', 'PERMISSION first')

    const card = page.locator('.permission-card')
    await expect(card).toContainText('In the background · Run the suite')
    await expect(capsule(page)).toContainText(/In the background ·\s*Needs you/)
    await expect(page.locator('.roster-row', { hasText: name })).toContainText('Needs you')

    // On Details it leads Threads with what the card asks, and opens the chat at the card.
    const details = await openDetails(page)
    const asking = row(details, 'Threads', 'Run the suite')
    await expect(asking).toHaveAttribute('data-work-status', 'needsYou')
    await expect(asking).toContainText(ASKS)
    await asking.locator('.work-row-open').click()
    await expect(card).toBeInViewport()

    // Answered like any card; the item then finishes, and its report comes back as the bot's message.
    await card.getByRole('button', { name: 'Allow once' }).click()
    await expect(capsule(page)).toHaveCount(0)
    await expect(page.locator('.transcript')).toContainText('Relayed:')
    const done = row(details, 'Recent', 'Run the suite')
    await expect(done).toHaveAttribute('data-work-status', 'done')
    await expect(done).toContainText('Done')
    await expect(done.getByRole('button', { name: 'Try again' })).toHaveCount(0)
    // An ended item is dropped like a waiting one.
    await hoverAction(done, 'Drop')
    await expect(section(details, 'Recent')).toHaveCount(0)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("a task's choice card and the file it shares say they come from the background, and only a press answers", async () => {
  const { browser, page } = await attach()
  const name = `Work choice e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    // The fixture's task asks `Which?` with the choices A and B, then sends a file.
    await addWork(page, botId, 'Pick one', 'ASK then SHARE')

    const card = page.locator('.decision-card', { hasText: 'Which?' })
    await expect(card).toContainText('In the background · Pick one')
    // No row for the user's own words: a typed message is for the bot in the chat, not the task.
    await expect(card.locator('.decision-own')).toHaveCount(0)
    await expect(capsule(page)).toContainText(/In the background ·\s*Needs you/)
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
    // The file the task shares afterwards is the bot's message, marked the same way.
    await expect(page.locator('.message.agent', { has: page.locator('.file-message') }).locator('.task-line')).toHaveText('In the background · Pick one')
    await expect(capsule(page)).toHaveCount(0)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('a working row stops once asked, and a stopped item tries again', async () => {
  const { browser, page } = await attach()
  const name = `Work stop e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name)
    await addWork(page, botId, 'Audit the repo', HOLD)

    const details = await openDetails(page)
    await hoverAction(row(details, 'Threads', 'Audit the repo'), 'Stop')
    const ask = page.getByRole('alertdialog')
    await expect(ask).toContainText('Stop the background work?')
    await ask.getByRole('button', { name: 'Cancel' }).click()
    await expect(row(details, 'Threads', 'Audit the repo')).toHaveAttribute('data-work-status', 'working')
    await hoverAction(row(details, 'Threads', 'Audit the repo'), 'Stop')
    await ask.getByRole('button', { name: 'Stop' }).click()

    const stopped = row(details, 'Recent', 'Audit the repo')
    await expect(stopped).toHaveAttribute('data-work-status', 'stopped')
    // Nothing is live or waiting: no threads, and no row of free places.
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

test("one view lists every bot's work, with the same rows and actions, and leads to a bot's Details", async () => {
  const { browser, page } = await attach()
  const name = `Work all e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // Shown only while some bot has live or waiting items.
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
    // The bot's threads, then its box: no titles, no free places, nothing that ended.
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

    // The view closes by itself when the last item ends, and its button goes with it.
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

test('Settings holds Instructions and Agent first, and Agents at once steps between 1 and 6', async () => {
  const { browser, page } = await attach()
  const name = `Work agents e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await openBot(page, name, 2)
    await page.getByRole('button', { name: `${name}, view details` }).click()
    const details = page.locator('.details-panel')
    await details.getByRole('tab', { name: 'Settings', exact: true }).click()
    await expect(details.locator('.card-section h3')).toHaveText(['Instructions', 'Agent', 'Profile', 'Workspace', 'Activity', 'Connectors', 'Skills'])

    const agents = details.locator('.value-row', { hasText: 'Agents at once' })
    await expect(agents.locator('.stepper-value')).toHaveText('2')
    await agents.getByRole('button', { name: 'Decrease' }).click()
    await expect(agents.locator('.stepper-value')).toHaveText('1')
    await expect(agents.getByRole('button', { name: 'Decrease' })).toBeDisabled()
    await expect.poll(async () => (await botState(page, botId!)).maxAgents).toBe(1)
    await agents.getByRole('button', { name: 'Increase' }).click()
    await expect.poll(async () => (await botState(page, botId!)).maxAgents).toBe(2)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})
