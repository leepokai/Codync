import { test, expect } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type Fallback = { backend: string; model?: string | null }

// #377: a fallback's row menu edits it in place, keeping its place in the chain.
test('a fallback row menu offers Edit, which keeps the step in its place', async () => {
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
    await page.locator('.details-panel').getByRole('tab', { name: 'Settings' }).click()
    await page.locator('.details-panel .fallback-menu').first().click()
    await expect(page.getByRole('menuitem', { name: 'Move down' })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: 'Edit' })).toBeVisible()
    await page.getByRole('menuitem', { name: 'Edit' }).click()
    const editing = page.locator('.sheet-page', { hasText: 'Edit fallback' })
    await expect(editing).toBeVisible()
    // Prefilled with the step's own agent and model; its own agent is offered again although the chain has it.
    const [agent, model] = [editing.locator('.agent-picker-choice').first(), editing.locator('.agent-picker-choice').nth(1)]
    await expect(agent).toContainText('Codex')
    await expect(model).toHaveText('e2e-model')

    // Another model (the agent's default, the one every agent lists), saved in the step's place.
    await model.click()
    await editing.locator('.agent-picker-item').first().click()
    await expect(model).not.toHaveText('e2e-model')
    await editing.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(editing).toHaveCount(0)

    const saved = async () => (await api<{ bot: { fallbacks: Fallback[] } }>(page, 'updateBot', { id: bot.id })).bot.fallbacks
    await expect.poll(async () => (await saved()).map((f) => [f.backend, f.model ?? null])).toEqual([['codex', null], ['gemini', null]])
    await expect(page.locator('.details-panel').getByText(/^Codex.* · Default$/)).toBeVisible()

    // Another agent for the second step: it stays second, on that agent.
    await page.locator('.details-panel .fallback-menu').nth(1).click()
    await page.getByRole('menuitem', { name: 'Edit' }).click()
    await expect(agent).toContainText('Gemini')
    await agent.click()
    await editing.locator('.agent-picker-item[aria-pressed="false"]').first().click()
    await expect(agent).not.toContainText('Gemini')
    await editing.getByRole('button', { name: 'Save', exact: true }).click()
    await expect(editing).toHaveCount(0)
    await expect.poll(async () => (await saved()).map((f) => f.backend)).toEqual(['codex', expect.not.stringMatching(/^(codex|gemini|kimi)$/)])
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
