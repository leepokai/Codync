import { test, expect, type Page } from '@playwright/test'
import { api, attach, echoBot, holdTask, setPref, withPrefs, workBot } from './sandbox.ts'

/**
 * Where the two scrollers are, read from the box nearest `scope`.
 *
 * The chat's view is `.chat-view`. It is `overflow: hidden`, which still makes it a scroll container,
 * so anything hanging out of its box counts toward its scrollHeight and lets the keyboard scroll it.
 * It must always be the same height as its content: nothing may be able to scroll it.
 */
const read = (page: Page, scope = '') => page.evaluate((selector) => {
  const root: ParentNode = selector ? document.querySelector(selector) ?? document : document
  const field = root.querySelector<HTMLElement>('.composer-field textarea')
  if (!field) return null
  const textarea = field as HTMLTextAreaElement
  const wrapper = textarea.closest<HTMLElement>('.composer-field')!
  const measure = wrapper.querySelector<HTMLElement>('.composer-measure')
  const dock = textarea.closest<HTMLElement>('.composer-dock')!
  const view = textarea.closest<HTMLElement>('.chat-view')!
  return {
    viewTop: Math.round(view.scrollTop),
    viewRoom: Math.round(view.scrollHeight - view.clientHeight),
    dockTop: Math.round(dock.getBoundingClientRect().top),
    fieldTop: Math.round(textarea.scrollTop),
    fieldMax: Math.round(textarea.scrollHeight - textarea.clientHeight),
    fieldHeight: Math.round(textarea.getBoundingClientRect().height),
    measureHeight: measure ? Math.round(measure.getBoundingClientRect().height) : 0,
    fieldOverflow: getComputedStyle(wrapper).overflowY,
    lines: textarea.value.split('\n').length,
    size: getComputedStyle(document.documentElement).getPropertyValue('--conversation').trim(),
  }
}, scope)

/**
 * A paste of several screens: the reported trigger is a lot of text, far past the eight lines the box
 * grows to. Anything shorter leaves the hidden copy no taller than the box and shows nothing.
 */
const hugePaste = (lines: number) => Array.from(
  { length: lines },
  (_, i) => `line ${i + 1}: lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore`,
).join('\n')

/** Puts the caret at the end of the draft, so the field's own scroll sits at its bottom. */
const caretToEnd = (page: Page, scope = '') => page.evaluate((selector) => {
  const root: ParentNode = selector ? document.querySelector(selector) ?? document : document
  const field = root.querySelector<HTMLTextAreaElement>('.composer-field textarea')!
  field.setSelectionRange(field.value.length, field.value.length)
  field.focus()
}, scope)

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
const settled = async (page: Page, scope = '') => {
  let last = (await read(page, scope))!
  for (let i = 0; i < 40; i++) {
    await page.waitForTimeout(150)
    const now = (await read(page, scope))!
    if (now.dockTop === last.dockTop && now.viewRoom === last.viewRoom) return now
    last = now
  }
  return last
}

/** Pastes `lines` lines into the box and leaves the caret at the end of them, as a paste does. */
const paste = async (page: Page, lines: number, scope = '') => {
  const root = scope ? page.locator(`${scope} .composer-field textarea`).first() : page.locator('.composer-field textarea').first()
  await root.fill(hugePaste(lines))
  await expect.poll(async () => (await read(page, scope))!.lines, { timeout: 15_000 }).toBe(lines)
  await caretToEnd(page, scope)
  return settled(page, scope)
}

/**
 * The reported sequence, in the reporter's own words: paste a large block into the message box, keep
 * the caret in it, press PageUp.
 *
 * What was expected is one of two things: the box pages through the text just pasted, or the chat
 * pages through its history. What happened instead was the whole box jumping to the top of the window,
 * stuck there with about two lines showing, and PageDown never bringing it back: the conversation scrolled
 * under the box and could not be scrolled back.
 *
 * The cause is the hidden copy the field measures its own height from. It is `position: absolute` with
 * `inset: 0 0 auto`, and it held the whole draft: a pasted page made it thousands of pixels tall, hanging
 * out of the composer and far past the bottom of the chat. The chat view is `overflow: hidden`, still a
 * scroll container, so that spill counted toward its scrollHeight and made the view scrollable from the
 * keyboard alone. Once the field's own text reached its top, the next PageUp chained into that scroll and
 * threw the conversation, box and all, to the top.
 *
 * So the assertions are: the chat's view is never scrollable, the box holds its place, and every PageUp
 * and PageDown is spent on the field's own text and nowhere else. It must hold at both ends of the text
 * size range, because the cap this used to have was arithmetic that had to match the field's own.
 */
test('a very large paste never lets PageUp scroll the chat behind the box', async () => {
  test.setTimeout(300_000)
  const { page } = await attach()
  await settle(page)
  const name = `Composer page up ${Date.now()}`
  // The test changes the text size, so it puts back the size the window had: a run must not leave a saved
  // setting on whatever the test used last.
  const restore = await withPrefs(page, { conversationFontSize: 12 })
  let botId: string | undefined
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    // One turn, so the chat has a row or two to sit behind the box.
    await api(page, 'send', { botId: bot.id, text: 'Say 1' })
    await expect.poll(async () => (await api<{ entries: { kind: string }[] }>(page, 'history', { botId: bot.id, limit: 6 })).entries.some((e) => e.kind !== 'user'), { timeout: 20_000 }).toBe(true)
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(page.locator('.composer-field textarea')).toBeVisible()

    // Both ends of the range the app offers (11 to 18): the field's eight lines are 130px at 12 and 194px
    // at 18, so a fix whose geometry is fixed instead of measured cannot pass both.
    for (const size of [12, 18]) {
      await setPref(page, 'conversationFontSize', size)
      const grown = await paste(page, 200)
      expect(grown.size, 'the text size the app says it is using').toBe(`${size}px`)
      expect(grown.fieldHeight, 'the box grew past its shortest height').toBeGreaterThan(60)

      // The copy is as tall as the whole paste; the field is not. That difference is the spill that used
      // to reach out of the box, so it must not be able to make the view behind it scroll.
      expect(grown.measureHeight, 'the hidden copy held the whole draft').toBeGreaterThan(grown.fieldHeight * 10)
      expect(grown.fieldMax, 'the field scrolls its own text').toBeGreaterThan(0)
      expect(grown.viewRoom, 'the paste left the conversation scrollable from the keyboard').toBeLessThanOrEqual(1)
      expect(grown.viewTop, 'the paste scrolled the conversation').toBe(0)

      // One PageUp first: it brings the caret's own line to the top of the box, and must still move
      // nothing behind it.
      await page.keyboard.press('PageUp')
      await page.waitForTimeout(250)
      const first = (await read(page))!
      expect(first.viewTop, 'the first PageUp moved the conversation').toBe(0)
      expect(first.viewRoom, 'the first PageUp made the conversation scrollable').toBeLessThanOrEqual(1)
      expect(Math.abs(first.dockTop - grown.dockTop), 'the first PageUp moved the box').toBeLessThanOrEqual(6)

      // Then pages of the box's own text, one after another, down to its top. Each is the field's,
      // and the box holds its place the whole way.
      let pressed = 0
      let now = (await read(page))!
      while (now.fieldTop > 0 && pressed < 80) {
        await page.keyboard.press('PageUp')
        await page.waitForTimeout(120)
        pressed++
        now = (await read(page))!
        expect(now.viewTop, 'PageUp chained out of the box into the conversation').toBe(0)
        expect(now.viewRoom, 'the conversation became scrollable while paging the box').toBeLessThanOrEqual(1)
        expect(Math.abs(now.dockTop - grown.dockTop), 'the box jumped away from the bottom').toBeLessThanOrEqual(6)
      }
      expect(now.fieldTop, 'the field reached the top of its own text').toBe(0)

      // And PageDown comes back down the field's own text, as it did before.
      await page.keyboard.press('PageDown')
      await expect.poll(async () => (await read(page))!.fieldTop, { timeout: 10_000 }).toBeGreaterThan(0)
      const down = (await read(page))!
      expect(down.viewTop, 'PageDown moved the conversation').toBe(0)
      expect(down.viewRoom, 'PageDown made the conversation scrollable').toBeLessThanOrEqual(1)
      expect(Math.abs(down.dockTop - grown.dockTop), 'PageDown left the box somewhere else').toBeLessThanOrEqual(6)

      // Shrinking the draft back to two lines, then growing it again: neither direction may leave the view
      // scrollable, and the box follows the draft the way it always did.
      const small = await paste(page, 2)
      expect(small.fieldHeight, 'the box shrank back to two lines').toBeLessThan(grown.fieldHeight)
      expect(small.dockTop, 'the box did not come back down').toBeGreaterThan(grown.dockTop)
      expect(small.viewRoom, 'shrinking left the conversation scrollable').toBeLessThanOrEqual(1)
      const regrown = await paste(page, 200)
      expect(regrown.viewRoom, 'growing the draft again left the conversation scrollable').toBeLessThanOrEqual(1)
      expect(regrown.viewTop, 'growing the draft again scrolled the conversation').toBe(0)
    }
  } finally {
    await restore()
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
  }
})

/**
 * The same box, in a thread's details panel: `SideChat` builds its own `.chat-view` and its own
 * `Composer`, so the same spill hung out of that view instead of the bot's. It is the same fix and
 * needs the same proof, because a clipped wrapper is what has to hold for both.
 */
test('a very large paste in the side chat leaves its own view unscrollable', async () => {
  test.setTimeout(300_000)
  const { page } = await attach()
  await settle(page)
  const name = `Side chat page up ${Date.now()}`
  const title = 'Side chat paste'
  let botId: string | undefined
  try {
    botId = (await workBot(page, name)).id
    await holdTask(page, botId, title)
    await page.locator('.nav-rail').getByRole('button', { name: 'Threads', exact: true }).click()
    await page.locator(`[data-thread-row="${title}"]`).click()
    await page.getByRole('button', { name: 'Conversation details', exact: true }).click()
    const panel = page.locator('.side-panel:not([inert])').filter({ visible: true })
    await expect(panel).toBeVisible()
    await panel.getByRole('button', { name: 'Open a view', exact: true }).click()
    await panel.locator('.panel-choice').filter({ hasText: 'Side chat' }).click()
    await expect(page.locator('.side-chat .composer-field textarea')).toBeVisible()

    const scope = '.side-chat'
    const grown = await paste(page, 200, scope)
    expect(grown.measureHeight, 'the hidden copy held the whole draft').toBeGreaterThan(grown.fieldHeight * 10)
    expect(grown.viewRoom, 'the paste left the side chat scrollable from the keyboard').toBeLessThanOrEqual(1)
    expect(grown.viewTop, 'the paste scrolled the side chat').toBe(0)

    let pressed = 0
    let now = (await read(page, scope))!
    while (now.fieldTop > 0 && pressed < 80) {
      await page.keyboard.press('PageUp')
      await page.waitForTimeout(120)
      pressed++
      now = (await read(page, scope))!
      expect(now.viewTop, 'PageUp chained out of the side chat box into its conversation').toBe(0)
      expect(now.viewRoom, 'the side chat became scrollable while paging the box').toBeLessThanOrEqual(1)
      expect(Math.abs(now.dockTop - grown.dockTop), 'the side chat box jumped away from the bottom').toBeLessThanOrEqual(6)
    }
    expect(now.fieldTop, 'the side chat field reached the top of its own text').toBe(0)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
  }
})
