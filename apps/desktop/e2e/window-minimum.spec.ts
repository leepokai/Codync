import { test, expect } from '@playwright/test'
import { chatWindowMinHeight, chatWindowMinWidth } from '../src/shared/chat-layout.ts'
import { api, attach, echoBot } from './sandbox.ts'

// The window's minimum is the page's floor, not the frame's: on Windows a frame-sized minimum left the
// page 664 × 435 and the composer below its bottom edge (#476). macOS hides the title bar, so the frame is
// the page and this passes there either way: it catches #476 against a Windows or Linux app.
test('the chat window shrunk to its minimum keeps the whole page and the composer', async () => {
  const { browser, page } = await attach()
  const name = `Minimum e2e ${Date.now()}`
  let botId: string | undefined
  const [width, height] = await page.evaluate(() => [outerWidth, outerHeight])
  try {
    botId = (await echoBot(page, name)).id
    await page.locator('.roster-row', { hasText: name }).click()
    // resizeTo sizes the frame and stops at the window's minimum, like dragging its corner.
    await page.evaluate(() => resizeTo(300, 200))
    await expect.poll(() => page.evaluate(() => [innerWidth, innerHeight])).toEqual([chatWindowMinWidth, chatWindowMinHeight])
    const composer = page.locator('.composer textarea')
    await expect(composer).toBeVisible()
    const hit = await composer.evaluate((e) => {
      const r = e.getBoundingClientRect()
      return document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2) === e
    })
    expect(hit).toBe(true)
  } finally {
    await page.evaluate(([w, h]) => resizeTo(w, h), [width, height])
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
