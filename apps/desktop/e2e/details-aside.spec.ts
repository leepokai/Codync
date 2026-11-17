import { test, expect, type CDPSession, type Page } from '@playwright/test'
import { resolve } from 'node:path'
import { api, attach, setPref } from './sandbox.ts'

// The window widths this view treats as compact / able to dock the panel (ThreadView's WIDE).
const COMPACT = 900
const DOCKED = 1084

async function resize(page: Page, width: number, cdp: CDPSession, height = 733) {
  await cdp.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 0, mobile: false })
  await expect.poll(() => page.evaluate(() => [window.innerWidth, window.innerHeight])).toEqual([width, height])
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
    // The title band is 60 px only until the pill carries a connection subtitle: it grows then, and
    // the aside starts under the band's own bottom, not under the old fixed 60 px, or it covers the
    // pill's last row (3 px at the default size, 14 px at the largest).
    const band = (await page.locator('.top-chrome').boundingBox())!
    expect(box.y).toBeGreaterThanOrEqual(band.y + band.height - 0.5)
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

test('the aside stays open, scrollable and closable at 680x500 with 18 px text, a long draft, an attachment and a queue', async () => {
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  const name = `Aside floor ${Date.now()}`
  let botId: string | undefined
  let skillId: string | undefined
  try {
    await setPref(page, 'conversationFontSize', 18)
    await resize(page, 680, cdp, 500)
    await awaitHost(page)
    await dismissReleaseSheet(page)

    const { skill } = await api<{ skill: { id: string } }>(page, 'installSkill', {
      name: `Aside Skill ${Date.now()}`, description: 'Checks the menu', instructions: 'Say hi.',
    })
    skillId = skill.id
    const steerAgent = resolve(__dirname, '../../../host/tests/fixtures/steer_agent.py')
    botId = (await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: `python3 -X utf8 -u "${steerAgent}" steer 60`, connectors: [], skills: [skill.id], notify: false,
    })).bot.id
    await page.locator('.roster-row', { hasText: name }).click()

    const field = page.locator('.composer textarea')
    await expect(field).toBeVisible()

    // Start a turn and queue six waiting messages so the queue tray reaches its full height.
    await api(page, 'send', { botId, text: 'first' })
    await expect.poll(async () => {
      const { entries } = await api<{ entries: { kind: string; data: { text?: string; status?: string } }[] }>(page, 'history', { botId })
      return entries.some((e) => e.kind === 'user' && e.data.text === 'first' && e.data.status === 'sent')
    }).toBe(true)
    for (let i = 1; i <= 6; i++) await api(page, 'send', { botId, text: `Waiting ${i}` })
    await expect(page.locator('.queued-row')).toHaveCount(6)

    // Drop a synthetic PNG onto the composer so the attachment tile adds its 88 px.
    await page.evaluate(() => {
      const bytes = Uint8Array.from([
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1,
        0, 0, 0, 1, 8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84,
        120, 156, 99, 248, 255, 255, 63, 0, 5, 254, 2, 254, 167, 53, 129, 132, 0,
        0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
      ])
      const dt = new DataTransfer()
      dt.items.add(new File([bytes], 'shot.png', { type: 'image/png' }))
      document.querySelector('.composer')!.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt }))
    })
    await expect(page.locator('.attachment-tile')).toBeVisible()

    // Eight-line draft at 18 px font so the dock exceeds 500 px - chromeHeight.
    const eightLines = Array.from({ length: 8 }, (_, i) => `Line ${i + 1} draft`).join('\n')
    await field.fill(eightLines)
    await expect.poll(async () => {
      const raw = await page.locator('.thread-view').evaluate((el) => getComputedStyle(el).getPropertyValue('--composer-height'))
      return parseFloat(raw)
    }).toBeGreaterThan(380)

    const pill = page.locator('.title-pill')
    const aside = page.locator('.details-aside')
    await pill.click()
    await expect(aside).toBeVisible()

    // The aside keeps its usable minimum height below the title band and stops above the dock with 0 px overlap.
    const band = (await page.locator('.top-chrome').boundingBox())!
    const box = (await aside.boundingBox())!
    const dockBox = (await page.locator('.composer-dock').boundingBox())!
    expect(box.y).toBeGreaterThanOrEqual(band.y + band.height - 0.5)
    expect(box.height).toBeGreaterThanOrEqual(179.5)
    expect(box.y + box.height).toBeLessThanOrEqual(dockBox.y + 1)

    // Its close button stays inside the viewport, unobstructed, and reachable by pointer.
    const close = aside.getByRole('button', { name: /Close details|关闭详情/ })
    await expect(close).toBeVisible()
    const closeBox = (await close.boundingBox())!
    expect(closeBox.y).toBeGreaterThanOrEqual(box.y)
    expect(closeBox.y + closeBox.height).toBeLessThanOrEqual(500)
    expect(closeBox.x + closeBox.width).toBeLessThanOrEqual(680)
    expect(await page.evaluate(([x, y]) => {
      const hit = document.elementFromPoint(x, y)
      return !!hit?.closest('.details-actions button')
    }, [closeBox.x + closeBox.width / 2, closeBox.y + closeBox.height / 2] as const)).toBe(true)

    // The details column scrolls inside its clamped height.
    const scroller = aside.locator('.details-scroll')
    await expect(scroller).toBeFocused()
    const beforeScroll = await scroller.evaluate((el) => ({ top: el.scrollTop, client: el.clientHeight, scroll: el.scrollHeight }))
    expect(beforeScroll.scroll).toBeGreaterThan(beforeScroll.client)
    await page.keyboard.press('PageDown')
    await expect.poll(() => scroller.evaluate((el) => el.scrollTop)).toBeGreaterThan(0)

    // Scrolling up inside the dock exposes Send now and Remove completely below the aside,
    // unobstructed and working without closing the details.
    await page.locator('.composer-dock-scroll').evaluate((el) => { el.scrollTop = 0 })
    const firstQueueRow = page.locator('.queued-row').first()
    const sendNow = firstQueueRow.getByRole('button', { name: /Send now|立即发送/ })
    const removeQueued = firstQueueRow.getByRole('button', { name: /Remove|移除/ })
    await expect(sendNow).toBeVisible()
    await expect(removeQueued).toBeVisible()
    const sendNowBox = (await sendNow.boundingBox())!
    const removeBox = (await removeQueued.boundingBox())!
    expect(sendNowBox.y).toBeGreaterThanOrEqual(box.y + box.height - 1)
    expect(removeBox.y).toBeGreaterThanOrEqual(box.y + box.height - 1)
    expect(removeBox.y + removeBox.height).toBeLessThanOrEqual(500)
    for (const btnBox of [sendNowBox, removeBox]) {
      expect(await page.evaluate(([x, y]) => {
        const hit = document.elementFromPoint(x, y)
        return !!hit?.closest('.queued-row button')
      }, [btnBox.x + btnBox.width / 2, btnBox.y + btnBox.height / 2] as const)).toBe(true)
    }
    await removeQueued.click()
    await expect(page.locator('.queued-row')).toHaveCount(5)
    await expect(aside).toBeVisible()

    // The 8-line textarea sits below the aside and stays editable without closing the details.
    await field.click()
    const fieldBox = (await field.boundingBox())!
    expect(fieldBox.y).toBeGreaterThanOrEqual(box.y + box.height - 1)
    await field.fill(`${eightLines}\nedited`)
    await expect(field).toHaveValue(`${eightLines}\nedited`)
    await expect(aside).toBeVisible()

    // Opening `/` inside the scrolled dock portals `.composer-menu` unclipped above the dock.
    await field.fill('/')
    const menu = page.locator('.composer-menu')
    await expect(menu).toBeVisible()
    const menuBox = (await menu.boundingBox())!
    expect(menuBox.y).toBeGreaterThanOrEqual(0)
    expect(menuBox.height).toBeGreaterThan(20)
    const optionBox = (await menu.getByRole('option').first().boundingBox())!
    expect(optionBox.y).toBeGreaterThanOrEqual(menuBox.y)
    expect(optionBox.y + optionBox.height).toBeLessThanOrEqual(menuBox.y + menuBox.height + 0.5)
    expect(await page.evaluate(([x, y]) => !!document.elementFromPoint(x, y)?.closest('.composer-menu'), [optionBox.x + optionBox.width / 2, optionBox.y + optionBox.height / 2] as const)).toBe(true)
    await field.press('Escape')
    await expect(menu).toHaveCount(0)
    await expect(aside).toBeVisible()

    // Clicking the close button closes the aside even with the full dock in place.
    await close.click()
    await expect(aside).toHaveCount(0)
  } finally {
    await setPref(page, 'conversationFontSize', 14).catch(() => {})
    await cdp.send('Emulation.clearDeviceMetricsOverride').catch(() => {})
    if (botId) {
      await api(page, 'stop', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    if (skillId) await api(page, 'removeSkill', { id: skillId }).catch(() => {})
    await browser.close()
  }
})
