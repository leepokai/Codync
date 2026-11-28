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

// A short window (a small laptop, or a window shrunk by hand) still holds the whole panel: its rail and list give up
// height instead of running off the window's edge, where the last agents could be neither seen nor clicked.
test('in a short window the model panel stays inside it, and its last agent can be picked', async () => {
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  const name = `Short picker e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await steerBot(page, name, 1)).id
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1024, height: 560, deviceScaleFactor: 1, mobile: false })
    await page.locator('.roster-row', { hasText: name }).click()
    await openDetails(page, 'Settings')
    await page.locator('.details-panel .model-picker-field').click()
    const panel = page.locator('.model-picker')
    await expect(panel).toBeVisible()
    const [box, height] = [await panel.boundingBox(), await page.evaluate(() => innerHeight)]
    expect(box!.y).toBeGreaterThanOrEqual(0)
    expect(box!.y + box!.height).toBeLessThanOrEqual(height)
    const last = page.locator('.model-picker .model-picker-agent:not(.model-picker-add)').last()
    await last.click()
    await expect(last).toHaveAttribute('aria-pressed', 'true')
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride').catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
