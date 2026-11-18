import { test, expect } from '@playwright/test'
import { api, attach, echoBot } from './sandbox.ts'

// A portrait 1080 × 1920 display scaled to 720 × 1280 points is 720 wide (#359):
// the chat window filling it shows the whole page, nothing cut off on the right.
const displayWidth = 720

test('the chat window page fits a 720-point-wide portrait display', async () => {
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  const name = `Narrow e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await echoBot(page, name)).id
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: displayWidth, height: 1200, deviceScaleFactor: 0, mobile: false })
    // A bot's chat: `.chat-window` also wraps the onboarding and host-error pages, which have no columns.
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(page.locator('.sidebar')).toBeVisible()
    await expect(page.locator('.detail')).toBeVisible()
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(displayWidth)
    await expect.poll(() => page.locator('.chat-window').evaluate((e) => e.getBoundingClientRect().width)).toBeLessThanOrEqual(displayWidth)
    await expect.poll(() => page.locator('.detail').evaluate((e) => e.getBoundingClientRect().right)).toBeLessThanOrEqual(displayWidth)
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride')
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
