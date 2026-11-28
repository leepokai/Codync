import { test, expect } from '@playwright/test'
import { api, attach, openDetails, showView } from './sandbox.ts'

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
    const panel = await openDetails(page, 'Settings')
    const advisor = panel.getByRole('switch').filter({ hasText: 'Advisor mode' })
    const threadsAgent = panel.getByText("Threads' agent", { exact: true }).locator('..')
    await expect(advisor).toHaveAttribute('aria-checked', 'false')
    await expect(threadsAgent).toHaveCount(0)

    // On: the threads start on the bot's own agent and fallbacks, saved as the form is.
    await advisor.click()
    await expect.poll(async () => (await saved()).threadAgent?.backend).toBe('kimi')
    expect((await saved()).threadFallbacks?.map((f) => f.backend)).toEqual(['codex'])

    // Another agent for the threads, from the model panel's rail; the chat keeps its own.
    await threadsAgent.locator('.model-picker-field').click()
    await page.locator('.model-picker .model-picker-agent[aria-pressed="false"]:not(.model-picker-add)').first().click()
    await page.keyboard.press('Escape')
    await expect.poll(async () => (await saved()).threadAgent?.backend).not.toBe('kimi')
    const picked = await saved()
    expect([picked.backend, picked.fallbacks.map((f) => f.backend)]).toEqual(['kimi', ['codex']])

    // + under Threads' fallbacks adds a step to their chain, not the chat's.
    const before = picked.threadFallbacks?.length ?? 0
    await panel.getByRole('button', { name: 'Add fallback' }).last().click()
    await expect.poll(async () => (await saved()).threadFallbacks?.length).toBe(before + 1)
    // The new step opens its model panel over the page; it closes as any panel does.
    await page.keyboard.press('Escape')
    const kept = await saved()
    expect(kept.fallbacks.map((f) => f.backend)).toEqual(['codex'])

    // Opened again, the form shows what was kept.
    await showView(panel, 'Details')
    await expect(advisor).toHaveCount(0)
    await showView(panel, 'Settings')
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
