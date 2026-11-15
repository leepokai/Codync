import { test, expect } from '@playwright/test'
import { attach } from './sandbox.ts'

// A portrait 1080 × 1920 display scaled to 720 × 1280 points is 720 wide (#359):
// the chat window filling it shows the whole page, nothing cut off on the right.
const displayWidth = 720

test('the chat window page fits a 720-point-wide portrait display', async () => {
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  try {
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: displayWidth, height: 1200, deviceScaleFactor: 0, mobile: false })
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(displayWidth)
    await expect.poll(() => page.locator('.chat-window').evaluate((e) => e.getBoundingClientRect().width)).toBeLessThanOrEqual(displayWidth)
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride')
    await browser.close()
  }
})
