import { test, expect, type CDPSession, type Locator } from '@playwright/test'
import { api, attach, echoBot, setPref } from './sandbox.ts'

// The compact details sheet is a card over the chat below 740 px of chat width (ChatView's WIDE).
// The viewport is emulated at 760x500 / 1200x760 here, so this checks layout, focus and hit testing,
// not the OS window's own bounds or its native hit test.
async function setViewport(cdp: CDPSession, width: number, height: number) {
  await cdp.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
  await new Promise((done) => setTimeout(done, 250)) // the app measures the chat with a ResizeObserver, then re-renders
}

const maskOf = (el: Locator) => el.evaluate((node) => {
  const style = getComputedStyle(node)
  return style.maskImage || style.webkitMaskImage
})

test('the compact details sheet scrolls from the keyboard and keeps its clicks', async () => {
  test.setTimeout(120_000)
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
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
    // Only what the bot has fills its Details page: Threads, Event box, Recent and Dev servers are left out while
    // they are empty, and what is left fits inside the sheet, so its column would not scroll at all. Routines do
    // show, and ten of their rows are taller than the card at 760x500.
    for (let i = 1; i <= 10; i++) {
      await api(page, 'saveRoutine', {
        botId, name: `Routine ${i}`, instruction: 'Report once', triggers: [{ type: 'webhook' }], timeoutSeconds: 30,
      })
    }
    await expect(page.locator('.roster-row', { hasText: name })).toBeVisible()
    await page.locator('.roster-row', { hasText: name }).first().click()
    await setViewport(cdp, 760, 500)

    // The title pill opens the sheet while the chat is narrow (ChatView's toggleDetails).
    await page.locator('.title-pill').click()
    const sheet = page.locator('.modal-content > .details-panel')
    await expect(sheet).toBeVisible()

    // The column scrolls, not the card: focus lands in the scroller, and PageDown/End move it.
    await expect.poll(() => page.evaluate(() => document.activeElement?.className ?? '')).toContain('details-scroll')
    const scroll = sheet.locator('.details-scroll')
    // The keys below scroll this column only if it overflows the card; a page that fits would leave them no work.
    await expect.poll(() => scroll.evaluate((el) => el.scrollHeight - el.clientHeight)).toBeGreaterThan(120)
    await expect.poll(() => scroll.evaluate((el) => el.scrollTop)).toBe(0)
    await page.keyboard.press('PageDown')
    await expect.poll(() => scroll.evaluate((el) => el.scrollTop)).toBeGreaterThan(0)
    await page.keyboard.press('End')
    await expect.poll(() => scroll.evaluate((el) => Math.abs(el.scrollTop - (el.scrollHeight - el.clientHeight)) < 2)).toBe(true)
    // A column focused for the keyboard draws no ring around the card, and its fade is a mask.
    expect(await scroll.evaluate((el) => getComputedStyle(el).outlineStyle)).toBe('none')
    expect(await maskOf(scroll)).toContain('linear-gradient')

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

    // A wider chat dismisses the sheet and hands the panel back to its dock, with the band it needs there.
    await setViewport(cdp, 1200, 760)
    await expect(page.locator('.modal-content > .details-panel')).toHaveCount(0)
    await page.locator('.title-pill').click()
    const docked = page.locator('.side-panel .details-panel')
    await expect(docked).toBeVisible()
    expect(await docked.locator('.details-drag').evaluate((el) => getComputedStyle(el).display)).toBe('block')
    expect(await docked.locator('.details-drag').evaluate((el) => getComputedStyle(el).height)).toBe('54px')
    expect(await docked.locator('.details-identity').evaluate((el) => getComputedStyle(el).paddingTop)).toBe('58px')
    expect(await maskOf(docked.locator('.details-scroll'))).toBe('none')
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride')
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
