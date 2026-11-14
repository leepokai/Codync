import { test, expect, type Page } from '@playwright/test'
import { api, attach, echoBot, setPref } from './sandbox.ts'

// The compact details sheet is a card over the chat below 740 px of thread width (ThreadView's WIDE).
// The viewport is emulated at 760x500 / 1200x760 here, so this checks layout, focus and hit testing,
// not the OS window's own bounds or its native hit test.
async function setViewport(page: Page, width: number, height: number) {
  const client = await page.context().newCDPSession(page)
  await client.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
  await page.waitForTimeout(250) // the app measures the thread with a ResizeObserver, then re-renders
}

test('the compact details sheet scrolls from the keyboard and keeps its clicks', async () => {
  test.setTimeout(120_000)
  const { browser, page } = await attach()
  // A sandbox with no saved account covers the window with the welcome; mark that choice the way
  // `attach` marks the star ask, so the sheet being tested is never behind it. (`welcome-continue`
  // would do it too, but the welcome is briefly re-rendered while the saved account is restored,
  // which makes a click there race the gate.)
  await setPref(page, 'macAccountOnboardingCompleted', true)
  await setPref(page, 'withoutAccount', true)
  if (await page.locator('.welcome').count()) await page.reload()
  await expect(page.locator('.welcome')).toHaveCount(0, { timeout: 15_000 })
  await expect(page.locator('.sidebar')).toBeVisible({ timeout: 15_000 })
  const name = `Details sheet ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await echoBot(page, name)).id
    await expect(page.locator('.roster-row', { hasText: name })).toBeVisible()
    await page.locator('.roster-row', { hasText: name }).first().click()
    await setViewport(page, 760, 500)

    // The title pill opens the sheet while the thread is narrow (ThreadView's toggleDetails).
    await page.locator('.title-pill').click()
    const sheet = page.locator('.modal-content > .details-panel')
    await expect(sheet).toBeVisible()

    // The column scrolls, not the card: focus lands in the scroller, and PageDown/End move it.
    await expect.poll(() => page.evaluate(() => document.activeElement?.className ?? '')).toContain('details-scroll')
    const scroll = sheet.locator('.details-scroll')
    await expect.poll(() => scroll.evaluate((el) => el.scrollTop)).toBe(0)
    await page.keyboard.press('PageDown')
    await expect.poll(() => scroll.evaluate((el) => el.scrollTop)).toBeGreaterThan(0)
    await page.keyboard.press('End')
    await expect.poll(() => scroll.evaluate((el) => Math.abs(el.scrollTop - (el.scrollHeight - el.clientHeight)) < 2)).toBe(true)
    // A column focused for the keyboard draws no ring around the card, and its fade is a mask.
    expect(await scroll.evaluate((el) => getComputedStyle(el).outlineStyle)).toBe('none')
    expect(await scroll.evaluate((el) => getComputedStyle(el).maskImage || (getComputedStyle(el) as unknown as { webkitMaskImage: string }).webkitMaskImage)).toContain('linear-gradient')

    // The sheet has no window drag band, and its top strip now reaches the tabs.
    expect(await sheet.locator('.details-drag').evaluate((el) => getComputedStyle(el).display)).toBe('none')
    expect(await sheet.locator('.details-identity').evaluate((el) => getComputedStyle(el).paddingTop)).toBe('20px')
    const onTab = await sheet.evaluate((panel) => {
      const scroller = panel.querySelector('.details-scroll') as HTMLElement
      const tab = panel.querySelector('[role="tab"]') as HTMLElement
      scroller.scrollTop += tab.getBoundingClientRect().top - panel.getBoundingClientRect().top - 2
      const r = tab.getBoundingClientRect()
      return Boolean(document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2)?.closest('[role="tab"]'))
    })
    expect(onTab).toBe(true)

    // A wider thread dismisses the sheet and hands the panel back to its dock, with the band it needs there.
    await setViewport(page, 1200, 760)
    await expect(page.locator('.modal-content > .details-panel')).toHaveCount(0)
    await page.locator('.title-pill').click()
    const docked = page.locator('.side-panel .details-panel')
    await expect(docked).toBeVisible()
    expect(await docked.locator('.details-drag').evaluate((el) => getComputedStyle(el).display)).toBe('block')
    expect(await docked.locator('.details-drag').evaluate((el) => getComputedStyle(el).height)).toBe('54px')
    expect(await docked.locator('.details-identity').evaluate((el) => getComputedStyle(el).paddingTop)).toBe('58px')
    expect(await docked.locator('.details-scroll').evaluate((el) => getComputedStyle(el).maskImage || (getComputedStyle(el) as unknown as { webkitMaskImage: string }).webkitMaskImage)).toBe('none')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
