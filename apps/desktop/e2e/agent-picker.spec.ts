import { test, expect } from '@playwright/test'
import { api, attach, openDetails, steerBot } from './sandbox.ts'

type Hello = { backends: { id: string; name: string }[] }

test("the bot's settings pick its agent and login in the model panel's rail, each login once", async () => {
  const { browser, page } = await attach()
  const name = `Agent picker e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // Clients key their agent lists by id, so the host lists each agent once.
    const { backends } = await api<Hello>(page, 'hello', {})
    const ids = backends.map((b) => b.id)
    expect(new Set(ids).size, ids.join(', ')).toBe(ids.length)
    expect(backends.filter((b) => b.name === 'Kiro CLI')).toHaveLength(1)

    botId = (await steerBot(page, name, 1)).id
    await page.locator('.roster-row', { hasText: name }).click()
    await openDetails(page, 'Settings')
    const field = page.locator('.details-panel .model-picker-field')
    await field.click()
    const rail = page.locator('.model-picker .model-picker-agent:not(.model-picker-add)')
    const labels = await rail.evaluateAll((seats) => seats.map((s) => s.getAttribute('aria-label')))
    expect(new Set(labels).size, labels.join(', ')).toBe(labels.length)

    for (const agent of ['Custom command', 'Kiro CLI']) {
      await page.locator(`.model-picker .model-picker-agent[aria-label^="${agent}"]`).first().click()
      await expect(field).toContainText(agent)
      await expect(page.locator('.model-picker .bot-command')).toHaveCount(agent === 'Custom command' ? 1 : 0)
    }
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
