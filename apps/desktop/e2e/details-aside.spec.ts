import { test, expect, type CDPSession, type Page } from '@playwright/test'
import { api, attach } from './sandbox.ts'

// The window widths this view treats as compact / able to dock the panel (ThreadView's WIDE).
const COMPACT = 900
const DOCKED = 1084

async function resize(page: Page, width: number, cdp: CDPSession) {
  await cdp.send('Emulation.setDeviceMetricsOverride', { width, height: 733, deviceScaleFactor: 0, mobile: false })
  await expect.poll(() => page.evaluate(() => window.innerWidth)).toBe(width)
}

/** A bot nothing runs, whose chat opens for the details. */
async function createBot(page: Page, name: string) {
  const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
    name, backend: 'custom', command: 'unused', connectors: [], skills: [], notify: false,
  })
  return bot.id
}

/** The window has connected to its host: a fresh sandbox answers only after its first snapshot. */
async function awaitHost(page: Page) {
  await expect.poll(() => page.evaluate(async () => {
    const host = (window as unknown as { codync?: { host?: { snapshot(): Promise<{ local?: unknown }> } } }).codync?.host
    try {
      return !!(await host?.snapshot())?.local
    } catch {
      return false
    }
  }), { timeout: 20_000 }).toBe(true)
}

/**
 * A sandbox that has not seen this release opens the What's new sheet over everything, and it swallows every
 * click the test makes: a developer's sandbox has usually dismissed it, a fresh one never has, and it appears a
 * moment after the window connects, so wait for it before deciding there is none.
 */
async function dismissReleaseSheet(page: Page) {
  // The details aside is a dialog too, so match the release sheet by its version label, not by role alone.
  const sheet = page.locator('[role="dialog"]:visible').filter({ has: page.locator('.whats-new-version') })
  await sheet.first().waitFor({ state: 'visible', timeout: 4_000 }).catch(() => {})
  if (!(await sheet.count())) return
  await sheet.first().getByRole('button', { name: /关闭|Close/ }).first().click()
  await expect(sheet).toHaveCount(0)
}

test('the compact details sit beside the chat and close from the message box or a menu', async () => {
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  const stamp = Date.now()
  const names = [`Aside Alpha ${stamp}`, `Aside Beta ${stamp}`]
  const group = `Aside Group ${stamp}`
  const made: string[] = []
  try {
    await resize(page, COMPACT, cdp)
    await awaitHost(page)
    await dismissReleaseSheet(page)
    for (const name of names) made.push(await createBot(page, name))
    const groupId = (await api<{ bot: { id: string } }>(page, 'createBot', { name: group, kind: 'group', members: made.slice(0, 2) })).bot.id
    made.push(groupId)
    await page.locator('.roster-row', { hasText: group }).click()
    const field = page.locator('.composer textarea')
    await expect(field).toBeVisible()
    const pill = page.locator('.title-pill')
    const aside = page.locator('.details-aside')

    await pill.click()
    await expect(aside).toBeVisible()
    // The scrolling column starts with the focus: PageDown and the arrow keys act on it, not on
    // the title pill that opened the details.
    await expect(page.locator('.details-aside .details-scroll')).toBeFocused()
    // Over the chat, with no dim, below the title band and above the message box.
    await expect(page.locator('.modal-dim')).toHaveCount(0)
    expect(await page.locator('.thread-view').evaluate((el) => el.closest('[inert]') === null)).toBe(true)
    const box = (await aside.boundingBox())!
    const composer = (await page.locator('.composer').boundingBox())!
    expect(box.y).toBeGreaterThanOrEqual(59)
    expect(box.y + box.height).toBeLessThanOrEqual(composer.y + 1)

    // Typing a reply keeps the details open; Escape closes them and leaves the draft.
    await field.click()
    await field.fill('answer while reading')
    await expect(aside).toBeVisible()
    await field.press('Escape')
    await expect(aside).toHaveCount(0)
    await expect(field).toHaveValue('answer while reading')

    // The `@` list opens above the details, and its own Escape takes the key first.
    await pill.click()
    await expect(aside).toBeVisible()
    await field.fill('@')
    const menu = page.locator('.composer-menu')
    await expect(menu).toBeVisible()
    const row = (await menu.getByRole('option').first().boundingBox())!
    expect(await page.evaluate(([x, y]) => !!document.elementFromPoint(x, y)?.closest('.composer-menu'), [row.x + row.width / 2, row.y + row.height / 2] as const)).toBe(true)
    await field.press('Escape')
    await expect(menu).toHaveCount(0)
    await expect(aside).toBeVisible()
    await field.press('Escape')
    await expect(aside).toHaveCount(0)

    // A keyboard close gives the focus back to the pill that opens the same details.
    await pill.click()
    await expect(aside).toBeVisible()
    await expect(page.locator('.details-aside .details-scroll')).toBeFocused()
    await page.keyboard.press('Escape')
    await expect(aside).toHaveCount(0)
    await expect.poll(() => page.evaluate(() => document.activeElement?.classList.contains('title-pill'))).toBe(true)

    // A click on the chat closes them, and the chat underneath worked all along.
    await pill.click()
    await expect(aside).toBeVisible()
    await page.mouse.click(300, 300)
    await expect(aside).toHaveCount(0)
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride').catch(() => {})
    for (const botId of made.reverse()) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test('the details dock beside the chat once the window is wide enough', async () => {
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  const name = `Details dock ${Date.now()}`
  let botId: string | undefined
  try {
    await resize(page, COMPACT, cdp)
    await awaitHost(page)
    await dismissReleaseSheet(page)
    botId = await createBot(page, name)
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(page.locator('.composer textarea')).toBeVisible()
    await page.locator('.title-pill').click()
    await expect(page.locator('.details-aside')).toBeVisible()
    // Widening the window docks the same open details instead of closing them.
    await resize(page, DOCKED, cdp)
    await expect(page.locator('.side-panel')).toBeVisible()
    await expect(page.locator('.details-aside')).toHaveCount(0)
    // Narrowing it again brings the aside back for what is still open.
    await resize(page, COMPACT, cdp)
    await awaitHost(page)
    await dismissReleaseSheet(page)
    await expect(page.locator('.details-aside')).toBeVisible()
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride').catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
