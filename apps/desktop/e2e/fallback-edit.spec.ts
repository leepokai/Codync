import { test, expect } from '@playwright/test'
import { api, attach, openDetails } from './sandbox.ts'

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
    const panel = await openDetails(page, 'Settings')
    // The menu moves and removes; editing is the row itself.
    await panel.locator('.fallback-menu').first().click()
    await expect(page.getByRole('menuitem', { name: 'Move down' })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: 'Edit' })).toHaveCount(0)
    await page.keyboard.press('Escape')

    // The first step's row opens its model panel: its own agent and model.
    const rows = panel.locator('.fallback-number').locator('..')
    const field = (i: number) => rows.nth(i).locator('.model-picker-field')
    const picker = page.locator('.model-picker')
    await expect(field(0)).toContainText('Codex')
    await expect(field(0)).toContainText('e2e-model')
    await field(0).click()
    // Another model (the agent's default, the one every agent lists), saved in the step's place.
    await picker.locator('.model-picker-row').first().click()
    await page.keyboard.press('Escape')
    const saved = async () => (await api<{ bot: { fallbacks: Fallback[] } }>(page, 'updateBot', { id: bot.id })).bot.fallbacks
    await expect.poll(async () => (await saved()).map((f) => [f.backend, f.model ?? null])).toEqual([['codex', null], ['gemini', null]])
    await expect(page.locator('.sheet-page')).toHaveCount(0) // no sheet anywhere

    // Another agent for the second step, from the panel's rail: it stays second, on that agent.
    await expect(field(1)).toContainText('Gemini')
    await field(1).click()
    await picker.locator('.model-picker-agent[aria-pressed="false"]:not(.model-picker-add)').first().click()
    await expect(field(1)).not.toContainText('Gemini')
    await page.keyboard.press('Escape')
    await expect.poll(async () => (await saved()).map((f) => f.backend)).toEqual(['codex', expect.not.stringMatching(/^(codex|gemini|kimi)$/)])

    // + adds a step at the end, already open on its model panel.
    await panel.getByRole('button', { name: 'Add fallback' }).click()
    await expect(rows).toHaveCount(3)
    await expect(field(2)).toHaveAttribute('aria-expanded', 'true')
    await expect(picker.locator('.model-picker-rail')).toBeVisible()
    await expect.poll(async () => (await saved()).length).toBe(3)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
