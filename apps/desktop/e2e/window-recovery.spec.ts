import { test, expect } from '@playwright/test'
import { attach } from './sandbox.ts'

/** Whether the window shows the app; a crashed page doesn't answer. */
async function showsTheApp(): Promise<boolean> {
  const { browser, page } = await attach()
  try {
    return await page.locator('.sidebar-account').waitFor({ timeout: 2000 }).then(() => true, () => false)
  } finally {
    await browser.close()
  }
}

// A few runs within a minute leave the window down, as it should: the reloads in a row count (renderer-recovery.ts).
test('a window whose page crashed loads it again', async () => {
  const { browser, page } = await attach()
  const crashed = page.waitForEvent('crash')
  const cdp = await page.context().newCDPSession(page)
  // It never answers: the page that would goes down.
  void cdp.send('Page.crash').catch(() => {})
  await crashed
  await browser.close()
  await expect.poll(showsTheApp, { timeout: 20_000 }).toBe(true)
})
