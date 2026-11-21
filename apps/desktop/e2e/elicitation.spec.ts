import { test, expect } from '@playwright/test'
import { resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach } from './sandbox.ts'

// An agent's own question (ACP `elicitation/create`) on the choice card: with options, only an option answers it, so
// the card has no own-words row; free text has no options and keeps the row.
test("an agent's question with options has no own-words row, free text keeps it", async ({}, testInfo) => {
  const { browser, page } = await attach()
  let botId: string | undefined
  try {
    const offline = page.getByRole('button', { name: 'Continue without an account', exact: false })
    await expect(page.locator('.sidebar-account').or(offline).first()).toBeVisible()
    if (await offline.isVisible()) await offline.click()
    const command = fixtureCommand(resolve(__dirname, '../../../host/tests/fixtures/elicitation_agent.py'))
    const name = `Question e2e ${Date.now()}`
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command, connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'ASK_CHOICE' })
    const choice = page.locator('.decision-card', { hasText: 'Which database?' }).last()
    await expect(choice.locator('button.decision-option')).toHaveCount(2)
    await expect(choice.locator('.decision-own')).toHaveCount(0)
    await choice.screenshot({ path: testInfo.outputPath('elicitation-options.png') })
    await choice.locator('button.decision-option', { hasText: 'sqlite' }).click()
    await expect(choice.locator('button.decision-option')).toHaveCount(0)
    await expect(page.locator('.transcript')).toContainText('Reply: {"action": "accept"')
    await api(page, 'send', { botId, text: 'ASK_TEXT' })
    const text = page.locator('.decision-card', { hasText: 'Which database?' }).last()
    await expect(text.locator('.decision-own')).toHaveCount(1)
    await expect(text.locator('button.decision-option')).toHaveCount(0)
    await text.screenshot({ path: testInfo.outputPath('elicitation-free-text.png') })
    await text.getByPlaceholder('Something else').fill('Postgres, please')
    await page.keyboard.press('Enter')
    await expect(page.locator('.transcript')).toContainText('Postgres, please"}}')
  } finally {
    if (botId) {
      await api(page, 'stop', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})
