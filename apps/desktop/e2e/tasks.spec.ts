import { test, expect, type Page } from '@playwright/test'
import { api, attach, taskBot, typeMessage } from './sandbox.ts'

/** Stops the bot's tasks and deletes it. */
async function cleanUp(page: Page, botId: string | undefined) {
  if (!botId) return
  await api(page, 'stopTask', { botId }).catch(() => {})
  await api(page, 'stop', { botId }).catch(() => {})
  await api(page, 'deleteBot', { botId }).catch(() => {})
}

test('a background task shows above the composer, opens its details, and stops once asked', async () => {
  const { browser, page } = await attach()
  const name = `Tasks e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await taskBot(page, name)
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    // HOLD keeps the task working until it is stopped.
    await api(page, 'send', { botId, text: 'START Audit the repo|HOLD until released' })

    const capsule = page.locator('.task-capsule')
    await expect(capsule).toBeVisible()
    await expect(capsule).toContainText('In the background ·')
    // Once it works it says what it does, or its title; the time counts beside it.
    await expect(capsule).not.toContainText('Starting…')
    await expect(capsule).toContainText(/\d+:\d\d/)
    // The chat itself is free again: the roster row says what goes on in the background.
    await expect(page.locator('.roster-row', { hasText: name })).toContainText('In the background:')
    // And it answers: a message typed under the capsule gets its reply while the task goes on.
    await typeMessage(page, 'are you there?')
    await expect(page.locator('.transcript')).toContainText('reply: are you there?')
    await expect(capsule).toBeVisible()

    await capsule.locator('.task-capsule-open').click()
    const details = page.getByRole('dialog')
    await expect(details).toContainText('Audit the repo')
    await expect(details).toContainText('Working')
    // Its own transcript: what it was told, as a quote.
    await expect(details.locator('.task-told').first()).toContainText('HOLD until released')
    await expect(details.locator('textarea')).toHaveCount(0)
    await details.getByRole('button', { name: 'Close' }).click()
    await expect(details).toHaveCount(0)
    // None of the task's transcript is in the chat.
    await expect(page.locator('.transcript')).not.toContainText('[Task]')

    await capsule.getByRole('button', { name: 'Stop' }).click()
    const ask = page.getByRole('alertdialog')
    await expect(ask).toContainText('Stop the background work?')
    await expect(ask).toContainText('What it already changed stays.')
    await ask.getByRole('button', { name: 'Cancel' }).click()
    await expect(capsule).toBeVisible()
    await capsule.getByRole('button', { name: 'Stop' }).click()
    await ask.getByRole('button', { name: 'Stop' }).click()
    await expect(capsule).toHaveCount(0)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

test("a task's approval card in the chat says it comes from the background", async () => {
  const { browser, page } = await attach()
  const name = `Task card e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await taskBot(page, name)
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'START Run the suite|PERMISSION first' })

    const card = page.locator('.permission-card')
    await expect(card).toContainText('In the background · Run the suite')
    const capsule = page.locator('.task-capsule')
    await expect(capsule).toContainText('Needs you')
    // Answered like any card; the task then finishes, and its report comes back as the bot's message.
    await card.getByRole('button', { name: 'Allow once' }).click()
    await expect(capsule).toHaveCount(0)
    await expect(page.locator('.transcript')).toContainText('Relayed:')
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})
