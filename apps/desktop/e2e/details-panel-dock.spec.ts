import { test, expect, type CDPSession } from '@playwright/test'
import { api, attach, echoBot, setPref } from './sandbox.ts'

// The details panel sits beside the chat whenever both fit (the chat narrows for it), and covers the chat only when
// they don't (useSidePanelWidth). The viewport is emulated here, so this checks layout, not the OS window's bounds.
async function setViewport(cdp: CDPSession, width: number, height: number) {
  await cdp.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
  await new Promise((done) => setTimeout(done, 250)) // the app measures the chat with a ResizeObserver, then re-renders
}

test('the details panel docks beside the chat when it fits and covers it when it does not', async () => {
  test.setTimeout(120_000)
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  await setPref(page, 'macAccountOnboardingCompleted', true)
  await setPref(page, 'withoutAccount', true)
  if (await page.locator('.welcome').count()) await page.reload()
  await expect(page.locator('.welcome')).toHaveCount(0, { timeout: 15_000 })
  await expect(page.locator('.sidebar')).toBeVisible({ timeout: 15_000 })
  const name = `Details panel ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await echoBot(page, name)).id
    await page.locator('.roster-row', { hasText: name }).first().click()
    const panel = page.locator('.side-panel .details-panel')
    const conversation = page.locator('.detail .chat-view > .conversation').first()

    // The width of the reported window: plenty of room, so the panel opens beside the chat, never as a card over it.
    await setViewport(cdp, 1100, 760)
    await page.locator('.title-pill').click()
    await expect(panel).toBeVisible()
    await expect(page.locator('.modal-content > .details-panel')).toHaveCount(0)
    expect(await conversation.evaluate((el) => el.hasAttribute('inert'))).toBe(false)
    const side = await page.locator('.side-panel').boundingBox()
    const chat = await conversation.boundingBox()
    expect(chat!.x + chat!.width).toBeLessThanOrEqual(side!.x + 1)
    expect(chat!.width).toBeGreaterThanOrEqual(340)

    // Too narrow for both: the panel covers the chat, which leaves the keyboard's reach.
    await setViewport(cdp, 700, 500)
    await expect(panel).toBeVisible()
    await expect.poll(() => conversation.evaluate((el) => el.hasAttribute('inert'))).toBe(true)
    await expect(page.locator('.modal-content > .details-panel')).toHaveCount(0)

    // Wide again: back beside the chat.
    await setViewport(cdp, 1100, 760)
    await expect.poll(() => conversation.evaluate((el) => el.hasAttribute('inert'))).toBe(false)
    await expect(panel).toBeVisible()
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride')
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
