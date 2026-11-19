import { test, expect, type Page } from '@playwright/test'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach } from './sandbox.ts'

/** How many of the bot's turns have ended with its final `Done.`. */
async function turnsDone(page: Page, botId: string) {
  const { entries } = await api<{ entries: { data: { text?: string; final?: boolean } }[] }>(page, 'history', { botId })
  return entries.filter((e) => e.data.text === 'Done.' && e.data.final).length
}

// A bot's `notify: false` update (#387) keeps the one card in the chat, and that card opens the new version.
test('a quiet artifact update shows no new card and the card opens the new version', async () => {
  test.setTimeout(60_000)
  const { browser, page } = await attach()
  const name = `Artifact quiet ${Date.now()}`
  let botId: string | undefined
  try {
    const agent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/artifact_agent.py')
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(agent), connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'quiet-new' })
    const cards = page.locator('.artifact-card')
    await expect(cards).toHaveCount(1)
    await expect.poll(() => turnsDone(page, botId!)).toBe(1)

    await api(page, 'send', { botId, text: 'quiet-update' })
    await expect.poll(() => turnsDone(page, botId!)).toBe(2)
    await expect(cards).toHaveCount(1)
    await cards.click()
    const layer = page.locator('.artifact-layer')
    await expect(layer.locator('iframe').contentFrame().getByText('Quiet fix')).toBeVisible()
    await layer.getByRole('button', { name: 'Close artifact', exact: true }).click()
    await expect(layer).toBeHidden()
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
