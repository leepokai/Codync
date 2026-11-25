import { test, expect, type Page } from '@playwright/test'
import { api, attach, echoBot } from './sandbox.ts'

/**
 * Where the two scrollers are: the chat's view, and the field the paste went into.
 *
 * The chat's view is `.chat-view`. It is `overflow: hidden`, which still makes it a scroll container,
 * so anything hanging out of its box counts toward its scrollHeight and lets the keyboard scroll it.
 * It must always be the same height as its content: nothing may be able to scroll it.
 */
const scrollers = (page: Page) => page.evaluate(() => {
  const view = document.querySelector<HTMLElement>('.chat-view')!
  const field = document.querySelector<HTMLElement>('.composer-field textarea')!
  const dock = document.querySelector<HTMLElement>('.composer-dock')!
  return {
    viewTop: Math.round(view.scrollTop),
    viewRoom: Math.round(view.scrollHeight - view.clientHeight),
    dockTop: Math.round(dock.getBoundingClientRect().top),
    fieldTop: Math.round(field.scrollTop),
    fieldMax: Math.round(field.scrollHeight - field.clientHeight),
    fieldHeight: Math.round(field.getBoundingClientRect().height),
  }
})

/** Puts the caret at the end of the draft, so the field's own scroll sits at its bottom. */
const caretToEnd = (page: Page) => page.evaluate(() => {
  const field = document.querySelector<HTMLElement>('.composer-field textarea') as HTMLTextAreaElement
  field.setSelectionRange(field.value.length, field.value.length)
  field.focus()
})

/** A window that came back after a restart, with whatever it put up on the way. */
const settle = async (page: Page) => {
  for (let i = 0; i < 40; i++) {
    const s = await page.evaluate(() => ({ c: !!document.querySelector('.welcome-continue'), nav: document.querySelectorAll('.bot-row').length, w: !!document.querySelector('.welcome') }))
    if (s.c) { await page.evaluate(() => document.querySelector<HTMLElement>('.welcome-continue')!.click()); await page.waitForTimeout(2500); continue }
    if (s.nav > 0 && !s.w) return
    await page.waitForTimeout(500)
  }
  throw new Error('The sandbox never left its welcome screen.')
}

/**
 * Waits for the box's own height transition to finish, so a reading taken straight after a paste is
 * the settled layout and not a frame of the growth animation (the box slides up as it grows).
 */
const settledBox = async (page: Page) => {
  let last = await scrollers(page)
  for (let i = 0; i < 40; i++) {
    await page.waitForTimeout(150)
    const now = await scrollers(page)
    if (now.dockTop === last.dockTop && now.viewRoom === last.viewRoom) return now
    last = now
  }
  return last
}

/**
 * The reported sequence, in the reporter's own words: paste a large block into the message box, leave
 * the caret in it, and press PageUp.
 *
 * What the reader expects is one of two things: the box pages through the text just pasted, or the
 * chat pages through its history. What happened instead was the whole box jumping to the top of the
 * window, stuck there with about two lines showing, and PageDown never bringing it back — the
 * conversation scrolled under the box and could not be scrolled back.
 *
 * The cause is the hidden copy the field measures its own height from. It is `position: absolute`
 * with `inset: 0 0 auto`, and it held the whole draft: a pasted page made it thousands of pixels
 * tall, hanging out of the composer and far past the bottom of the chat. The chat view is
 * `overflow: hidden` — still a scroll container — so that spill counted toward its scrollHeight and
 * made the view scrollable from the keyboard alone. Once the field's own text reached its top, the
 * next PageUp chained into that scroll and threw the conversation, box and all, to the top.
 *
 * So the assertions are: the chat's view is never scrollable, the box never moves, and every PageUp
 * and PageDown is spent on the field's own text and nowhere else.
 */
test('a large paste leaves PageUp to the box, never the conversation behind it', async () => {
  test.setTimeout(180_000)
  const { page } = await attach()
  await settle(page)
  const name = `Composer page up ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    // One turn, so the chat has a row or two to sit behind the box.
    await api(page, 'send', { botId: bot.id, text: 'Say 1' })
    await expect.poll(async () => (await api<{ entries: { kind: string }[] }>(page, 'history', { botId: bot.id, limit: 6 })).entries.some((e) => e.kind !== 'user'), { timeout: 20_000 }).toBe(true)
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(page.locator('.composer-field textarea')).toBeVisible()

    const draft = Array.from({ length: 120 }, (_, i) => `line ${i + 1} of a long pasted message`).join('\n')
    await page.locator('.composer-field textarea').fill(draft)
    // The field grows to eight lines and then stops; the paste is far past that.
    await expect.poll(async () => (await scrollers(page)).fieldMax, { timeout: 15_000 }).toBeGreaterThan(0)
    // The box's height transition settles; the view must never gain a scroll of its own.
    await expect.poll(async () => (await scrollers(page)).viewRoom, { timeout: 15_000 }).toBeLessThanOrEqual(1)

    await caretToEnd(page)
    await expect.poll(async () => (await scrollers(page)).fieldTop, { timeout: 10_000 }).toBeGreaterThan(0)

    const before = await settledBox(page)
    const fieldStep = Math.round(before.fieldHeight * 0.87)

    // The first PageUp is spent bringing the caret's own line to the top of the box (the standard
    // behaviour when the caret sits at the end). It must still move nothing behind the box.
    await page.keyboard.press('PageUp')
    await page.waitForTimeout(200)
    const first = await scrollers(page)
    expect(first.viewTop, 'the first PageUp moved the conversation').toBe(0)
    expect(Math.abs(first.dockTop - before.dockTop), 'the first PageUp moved the box').toBeLessThanOrEqual(6)
    expect(first.viewRoom, 'the first PageUp made the conversation scrollable').toBeLessThanOrEqual(1)

    // The next ones page the box's own text, and only that.
    const next = await scrollers(page)
    await page.keyboard.press('PageUp')
    await expect.poll(async () => (await scrollers(page)).fieldTop, { timeout: 10_000 })
      .toBeLessThan(next.fieldTop)
    const after = await scrollers(page)
    expect(after.fieldTop, 'the box scrolled its own text up').toBeLessThan(before.fieldTop)
    expect(after.viewTop, 'the PageUp moved the conversation').toBe(0)
    // The box may not slide at all: it is where it was, and the view behind it has not moved.
    expect(Math.abs(after.dockTop - before.dockTop), 'the box stayed where it belongs').toBeLessThanOrEqual(6)
    expect(after.viewRoom, 'the conversation became scrollable').toBeLessThanOrEqual(1)

    // Pages on, until the field's own text is at its top. The box still may not move, and the
    // conversation still may not scroll: this is the press that used to chain out of the field.
    let pressed = 0
    let now = await scrollers(page)
    while (now.fieldTop > 0 && pressed < 40) {
      await page.keyboard.press('PageUp')
      await page.waitForTimeout(100)
      pressed++
      now = await scrollers(page)
      expect(now.viewTop, 'PageUp chained out of the box into the conversation').toBe(0)
      expect(Math.abs(now.dockTop - before.dockTop), 'the box jumped away from the bottom').toBeLessThanOrEqual(6)
      expect(now.viewRoom, 'the conversation became scrollable while paging the box').toBeLessThanOrEqual(1)
    }
    expect(now.fieldTop, 'the field reached the top of its own text').toBe(0)
    expect(now.viewTop, 'PageUp chained out of the box into the conversation').toBe(0)
    expect(Math.abs(now.dockTop - before.dockTop), 'the box jumped away from the bottom').toBeLessThanOrEqual(6)

    // And PageDown comes back down the field's own text, as it did before.
    await page.keyboard.press('PageDown')
    await expect.poll(async () => (await scrollers(page)).fieldTop, { timeout: 10_000 }).toBeGreaterThan(0)
    const down = await scrollers(page)
    expect(down.viewTop, 'PageDown moved the conversation').toBe(0)
    expect(Math.abs(down.dockTop - before.dockTop), 'PageDown left the box somewhere else').toBeLessThanOrEqual(6)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
  }
})
