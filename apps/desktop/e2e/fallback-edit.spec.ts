import { test, expect } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type Fallback = { backend: string; model?: string | null }

// #377: a fallback is edited in place in the Agent card, like the bot's own agent, keeping its place in the chain.
test('a fallback row is picked in place and keeps its place in the chain', async () => {
  const { browser, page } = await attach()
  const name = `Fallback edit e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'kimi', connectors: [], skills: [], notify: false,
      fallbacks: [
        { backend: 'codex', model: 'e2e-model', modelOptions: {} },
        { backend: 'gemini', model: null, modelOptions: {} },
      ],
    })
    botId = bot.id

    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: `${name}, view details` }).click()
    const panel = page.locator('.details-panel')
    await panel.getByRole('tab', { name: 'Settings' }).click()
    // The menu moves and removes; editing is the row itself.
    await panel.locator('.fallback-menu').first().click()
    await expect(page.getByRole('menuitem', { name: 'Move down' })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: 'Edit' })).toHaveCount(0)
    await page.keyboard.press('Escape')

    // The first step's summary opens its pickers in place: its own agent and model.
    const rows = panel.locator('.fallback-number').locator('..')
    const first = rows.nth(0)
    await expect(first.locator('.agent-picker-summary')).toContainText('Codex')
    await expect(first.locator('.agent-picker-summary')).toContainText('e2e-model')
    await first.locator('.agent-picker-summary').click()
    // Another model (the agent's default, the one every agent lists), saved in the step's place.
    await first.locator('.agent-picker-item').first().click()
    const saved = async () => (await api<{ bot: { fallbacks: Fallback[] } }>(page, 'updateBot', { id: bot.id })).bot.fallbacks
    await expect.poll(async () => (await saved()).map((f) => [f.backend, f.model ?? null])).toEqual([['codex', null], ['gemini', null]])
    await expect(page.locator('.sheet-page')).toHaveCount(0) // no sheet anywhere

    // Another agent for the second step: it stays second, on that agent.
    const second = rows.nth(1)
    await second.locator('.agent-picker-summary').click()
    const agent = second.locator('.agent-picker-choice').first()
    await expect(agent).toContainText('Gemini')
    await agent.click()
    await second.locator('.agent-picker-item[aria-pressed="false"]').first().click()
    await expect(agent).not.toContainText('Gemini')
    await expect.poll(async () => (await saved()).map((f) => f.backend)).toEqual(['codex', expect.not.stringMatching(/^(codex|gemini|kimi)$/)])

    // + adds a step at the end, already open on its pickers.
    await panel.getByRole('button', { name: 'Add fallback' }).click()
    await expect(rows).toHaveCount(3)
    await expect(rows.nth(2).locator('.agent-picker-choice').first()).toBeVisible()
    await expect.poll(async () => (await saved()).length).toBe(3)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
