import { test, expect } from '@playwright/test'
import { resolve } from 'node:path'
import { api, attach, setPref } from './sandbox.ts'

test('a lost goal action reply is surfaced without replaying work or stopping a replacement', async () => {
  const { browser, page } = await attach()
  try {
    const results = await page.evaluate(async () => {
      const storeModule = '/store/bot-store.ts'
      const clientModule = '/client/host-client.ts'
      const { BotStore } = await import(/* @vite-ignore */ storeModule)
      const { HostError } = await import(/* @vite-ignore */ clientModule)
      const results = []
      for (const goal of ['Ship once', '', null]) {
        const store = Object.create(BotStore.prototype)
        let calls = 0
        store.ready = async () => ({ call: async () => {
          calls++
          // The host applied the first call, but its reply was lost. A replay would apply it again.
          if (calls === 1) throw new HostError(0, 'reply lost after acceptance')
          return {}
        } })
        let refused = false
        try {
          if (goal === null) await store.stopGoal('bot')
          else await store.setGoal('bot', goal, 2, 1)
        } catch { refused = true }
        results.push({ goal, calls, refused })
      }
      return results
    })
    expect(results).toEqual([
      { goal: 'Ship once', calls: 1, refused: true },
      { goal: '', calls: 1, refused: true },
      { goal: null, calls: 1, refused: true },
    ])
  } finally { await browser.close() }
})

test('goal controls start a bounded run and show completion and stop', async () => {
  const { browser, page } = await attach()
  let botId: string | undefined
  const preferences = await page.evaluate(() => Object.fromEntries(
    ['appLanguage', 'computerAccessSetup', 'conversationFontSize'].map((key) => [key, localStorage.getItem(key)]),
  ))
  try {
    await setPref(page, 'appLanguage', 'en')
    const goOn = page.getByRole('button', { name: /^Continue without an account/ })
    if (await goOn.isVisible()) await goOn.click()
    await setPref(page, 'computerAccessSetup', 'later')
    await page.reload()
    const agent = resolve(__dirname, '../../../host/tests/fixtures/team_agent.py')
    const name = `Goal e2e ${Date.now()}`
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: `python -X utf8 -u "${agent}"`,
      connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: `${name}, view details`, exact: true }).click()
    const form = page.locator('.goal-section')
    await form.getByRole('textbox', { name: 'Goal', exact: true }).fill('GOAL_DONE Ship the notes')
    await form.getByRole('spinbutton', { name: 'Maximum turns' }).fill('4')
    await form.getByRole('button', { name: 'Start goal', exact: true }).click()
    await expect(page.locator('.goal-banner')).toContainText('Goal complete', { timeout: 30000 })
    await expect(page.locator('.goal-banner')).toContainText('2/4')
    await form.getByRole('textbox', { name: 'Goal', exact: true }).fill('GOAL_BLOCK wait')
    await form.getByRole('button', { name: 'Start goal', exact: true }).click()
    await expect(page.locator('.goal-banner')).toContainText('GOAL_BLOCK wait')
    await page.locator('.goal-banner').getByRole('button', { name: 'Stop goal', exact: true }).click()
    await expect(page.locator('.goal-banner')).toContainText('Stopped')
    await setPref(page, 'conversationFontSize', 12)
    const summary = page.locator('.goal-summary > span').last()
    const originalSize = await summary.evaluate((el) => parseFloat(getComputedStyle(el).fontSize))
    await setPref(page, 'conversationFontSize', 18)
    await expect.poll(() => summary.evaluate((el) => parseFloat(getComputedStyle(el).fontSize))).toBeGreaterThan(originalSize)
    await expect(form.getByRole('textbox', { name: 'Goal', exact: true })).toHaveCSS('font-size', '18px')
    await page.screenshot({ path: 'test-results/goal-stopped.png' })
    await setPref(page, 'appLanguage', 'zh')
    await expect(page.locator('.goal-banner').getByRole('button', { name: '清除目标', exact: true })).toBeVisible()
    await page.screenshot({ path: 'test-results/goal-stopped-zh.png' })
    await page.locator('.goal-banner').getByRole('button', { name: '清除目标', exact: true }).click()
    await expect(page.locator('.goal-banner')).toHaveCount(0)
  } finally {
    if (botId) { await api(page, 'stop', { botId }); await api(page, 'deleteBot', { botId }) }
    await page.evaluate((saved) => {
      for (const [key, value] of Object.entries(saved)) {
        if (value === null) localStorage.removeItem(key)
        else localStorage.setItem(key, value)
        window.dispatchEvent(new StorageEvent('storage', { key, newValue: value }))
      }
    }, preferences)
    await browser.close()
  }
})
