import { test, expect } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type Fallback = { backend: string; model?: string | null }
type Bot = { backend: string; fallbacks: Fallback[]; threadAgent: Fallback | null; threadFallbacks: Fallback[] | null }

// Advisor mode: in the Agent card, the bot's threads get an agent and fallbacks of their own, apart from the chat's.
test("Advisor mode sets the threads' own agent and fallbacks, keeps them, and turns off", async () => {
  const { browser, page } = await attach()
  const name = `Advisor e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'kimi', connectors: [], skills: [], notify: false,
      fallbacks: [{ backend: 'codex', model: null, modelOptions: {} }],
    })
    botId = bot.id
    const saved = async () => (await api<{ bot: Bot }>(page, 'updateBot', { id: bot.id })).bot

    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: `${name}, view details` }).click()
    const panel = page.locator('.details-panel')
    await panel.getByRole('tab', { name: 'Settings' }).click()
    const advisor = panel.getByRole('switch').filter({ hasText: 'Advisor mode' })
    const threadsAgent = panel.getByText("Threads' agent", { exact: true }).locator('..')
    await expect(advisor).toHaveAttribute('aria-checked', 'false')
    await expect(threadsAgent).toHaveCount(0)

    // On: the threads start on the bot's own agent and fallbacks, saved as the form is.
    await advisor.click()
    await expect.poll(async () => (await saved()).threadAgent?.backend).toBe('kimi')
    expect((await saved()).threadFallbacks?.map((f) => f.backend)).toEqual(['codex'])

    // Another agent for the threads, picked in place; the chat keeps its own.
    await threadsAgent.locator('.agent-picker-summary').click()
    await threadsAgent.locator('.agent-picker-choice').first().click()
    await threadsAgent.locator('.agent-picker-item[aria-pressed="false"]').first().click()
    await expect.poll(async () => (await saved()).threadAgent?.backend).not.toBe('kimi')
    const picked = await saved()
    expect([picked.backend, picked.fallbacks.map((f) => f.backend)]).toEqual(['kimi', ['codex']])

    // + under Threads' fallbacks adds a step to their chain, not the chat's.
    const before = picked.threadFallbacks?.length ?? 0
    await panel.getByRole('button', { name: 'Add fallback' }).last().click()
    await expect.poll(async () => (await saved()).threadFallbacks?.length).toBe(before + 1)
    const kept = await saved()
    expect(kept.fallbacks.map((f) => f.backend)).toEqual(['codex'])

    // Opened again, the form shows what was kept.
    await panel.getByRole('tab', { name: 'Details' }).click()
    await expect(advisor).toHaveCount(0)
    await panel.getByRole('tab', { name: 'Settings' }).click()
    await expect(advisor).toHaveAttribute('aria-checked', 'true')
    await expect(threadsAgent).toBeVisible()
    expect((await saved()).threadAgent).toEqual(kept.threadAgent)

    // Off: no threads' agent or chain is kept.
    await advisor.click()
    await expect(threadsAgent).toHaveCount(0)
    await expect.poll(async () => (await saved()).threadAgent).toBeNull()
    expect((await saved()).threadFallbacks).toEqual([])
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
