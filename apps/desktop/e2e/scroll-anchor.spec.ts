import { test, expect, type Page } from '@playwright/test'
import { api, attach, echoBot } from './sandbox.ts'

// `mode_agent.py` answers each message with its text, so a chat of a few dozen turns is quick to build.
type Entry = { kind: string; data: { text?: string } }

/** Sends `text` and waits for the echo bot's answer. */
async function say(page: Page, botId: string, text: string) {
  await api(page, 'send', { botId, text })
  await expect.poll(async () => (await api<{ entries: Entry[] }>(page, 'history', { botId, limit: 6 }))
    .entries.some((e) => e.kind !== 'user' && e.data.text?.includes(text)), { timeout: 20_000 }).toBe(true)
}

/** Where the transcript sits, how far it can scroll, and how many rows it shows. */
const place = (page: Page) => page.evaluate(() => {
  const el = document.querySelector<HTMLElement>('.transcript')!
  return { top: Math.round(el.scrollTop), max: Math.round(el.scrollHeight - el.clientHeight), rows: el.querySelectorAll('[data-entry-id]').length }
})

/** Puts the keyboard's caret in the rows, as clicking a message does, so the paging keys move the chat. */
const focusRows = (page: Page) => page.evaluate(() => {
  const el = document.querySelector<HTMLElement>('.transcript')!
  ;(document.activeElement as HTMLElement | null)?.blur()
  el.setAttribute('tabindex', '0')
  el.focus()
})

/** The height the message box floats at, which is the room the rows leave under their last one. */
const composerHeight = (page: Page) => page.evaluate(() => Math.round(document.querySelector<HTMLElement>('.composer:not(.thread-chat .composer)')!.getBoundingClientRect().height))

/**
 * The row straddling the top edge of the view, and how far below that edge it starts. A reader paging up keeps
 * this row in front of them; earlier rows arriving above it without being paid for throw it off the view.
 */
const topRow = (page: Page) => page.evaluate(() => {
  const el = document.querySelector<HTMLElement>('.transcript')!
  const view = el.getBoundingClientRect().top
  const rows = [...el.querySelectorAll<HTMLElement>('[data-entry-id]')]
  const straddling = rows.find((row) => row.getBoundingClientRect().bottom > view + 2)
  return straddling ? { id: straddling.dataset.entryId!, offset: Math.round(straddling.getBoundingClientRect().top - view) } : null
})

/** Where a row sits relative to the view's top edge now, or null when it is no longer shown. */
const offsetOf = (page: Page, id: string) => page.evaluate((id) => {
  const el = document.querySelector<HTMLElement>(`[data-entry-id="${id}"]`)
  if (!el) return null
  return Math.round(el.getBoundingClientRect().top - document.querySelector<HTMLElement>('.transcript')!.getBoundingClientRect().top)
}, id)

/** One page of the reader's own: what the view moves by on PageUp or PageDown. */
const pageStep = (page: Page) => page.evaluate(() => Math.round(document.querySelector<HTMLElement>('.transcript')!.clientHeight * 0.87))

/** Scrolls the transcript to `top`, as the reader's own PageUp or wheel does. */
const scrollTo = (page: Page, top: number) => page.evaluate((top) => {
  document.querySelector<HTMLElement>('.transcript')!.scrollTop = top
}, top)

test('a page of earlier rows asked for at the newest does not move the reader afterwards', async () => {
  test.setTimeout(180_000) // Twenty-six turns go by first.
  const { page } = await attach()
  const name = `Scroll anchor e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    // More messages than one page of the newest, so the shown rows stop short of the computer's.
    for (let i = 1; i <= 26; i++) await say(page, bot.id, `Say ${i}`)
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(page.locator('.load-earlier')).toBeAttached()
    await expect.poll(async () => (await place(page)).rows, { timeout: 20_000 }).toBe(40)

    // "Load earlier messages" asks for a page while the reader is still at the newest, where the view is brought to
    // the bottom after every render. A place taken there used to outlive the render that took it, and the next render
    // after the reader left the bottom applied it, overriding the top the reader had chosen. Dispatched rather than
    // clicked: the button sits above the rows, and a click would scroll the transcript first, which is not what this
    // pins.
    await page.locator('.load-earlier').dispatchEvent('click')
    await expect.poll(async () => (await place(page)).rows, { timeout: 20_000 }).toBeGreaterThan(45)

    // The reader scrolls up to a place of their own. It must stay theirs.
    await scrollTo(page, 400)
    await expect.poll(async () => (await place(page)).top, { timeout: 10_000 }).toBeGreaterThanOrEqual(350)
    const after = await place(page)
    expect(after.top).toBeLessThan(450)
    // And they can still scroll on, so nothing was left to pull them back.
    await scrollTo(page, 900)
    await expect.poll(async () => (await place(page)).top, { timeout: 10_000 }).toBeGreaterThanOrEqual(850)
    expect((await place(page)).top).toBeLessThan(950)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
  }
})

/**
 * The reported sequence, in the reporter's own order: a large paste into the message box first, then paging up
 * through the chat, a PageDown and a PageUp, another chat and back, and paging up again.
 *
 * Two things have to hold at every step. A box that grows must not throw the rows to their top (the rows change
 * height under a reader whenever the box, a picture or the working line changes). And when a page of earlier rows
 * arrives, the row the reader was reading must stay in front of them: it may move by the reader's own page step,
 * and never jump a page's worth further up — which is what an arrival that is not paid for does.
 */
test('a paste first, then PageUp through the history, keeps the row the reader was on', async () => {
  test.setTimeout(240_000) // Fifty-five turns go by first, then the paging.
  const { page } = await attach()
  const name = `Scroll anchor steps ${Date.now()}`
  const other = `Scroll anchor other ${Date.now()}`
  let botId: string | undefined
  let otherId: string | undefined
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    // Three pages of rows beyond the newest: the first, then two later arrivals of earlier rows.
    for (let i = 1; i <= 55; i++) await say(page, bot.id, `Say ${i} · padding text for a realistic bubble`)
    await page.locator('.roster-row', { hasText: name }).click()
    await expect.poll(async () => (await place(page)).rows, { timeout: 30_000 }).toBe(40)

    // Step 1: the large paste. The box grows and the rows' content grows with it (their bottom padding is the room
    // the box leaves). The paste must leave the reader where they are: at the newest at the newest, and nowhere near
    // the rows' top.
    const boxBefore = await composerHeight(page)
    const draft = Array.from({ length: 60 }, (_, i) => `line ${i + 1} of a long pasted message`).join('\n')
    await page.locator('.composer:not(.thread-chat .composer) textarea').fill(draft)
    await expect.poll(async () => composerHeight(page), { timeout: 10_000 }).toBeGreaterThan(boxBefore + 40)
    const pasted = await place(page)
    expect(pasted.top, 'the paste did not throw the rows to their top').toBeGreaterThan(2)
    // The rows' room under the last one shrank with the box, so the newest row is still in front of the reader.
    expect((await topRow(page))!.offset).toBeGreaterThan(-200)

    // Steps 2-4: paging up from the rows, and back down again.
    await focusRows(page)
    const step = await pageStep(page)
    let was = (await place(page)).top
    expect(was, 'the rows open at their end').toBeGreaterThan(1)

    /**
     * One PageUp, measured by the row the reader was on: that row must end up about one page further up the
     * view — less only when they were nearer the top than a page — and no further, whichever page of earlier
     * rows arrives on the way. A view thrown to its top takes the row right off the screen, or leaves it a
     * page or more away.
     */
    const pageUp = async () => {
      const held = await topRow(page)
      const rowsBefore = (await place(page)).rows
      const up = Math.min(step, was)
      await page.keyboard.press('PageUp')
      await expect.poll(async () => {
        const now = await offsetOf(page, held!.id)
        return now === null ? -10_000 : now - held!.offset
      }, { timeout: 10_000 }).toBeGreaterThanOrEqual(up - 150)
      const now = await offsetOf(page, held!.id)
      expect(now, 'the row the reader was on is still shown').not.toBeNull()
      expect(now! - held!.offset, 'the page moved the view no further than the reader').toBeLessThanOrEqual(up + 150)
      was = (await place(page)).top
      const arrived = (await place(page)).rows > rowsBefore
      // An arrival is paid for: the reader keeps their place, so the view is nowhere near its top.
      if (arrived) expect(was, 'the arrival left the view at its top').toBeGreaterThan(200)
      return arrived
    }

    // PageUp, page by page, until a page of earlier rows arrives above the reader.
    let arrivals = 0
    for (let press = 1; press <= 4 && arrivals === 0; press++) if (await pageUp()) arrivals++
    expect(arrivals, 'a page of earlier rows arrived while paging up').toBeGreaterThan(0)

    // Step 4: PageDown comes back by about a page, and PageUp goes up again. Nothing pins the view down.
    const pageDown = async () => {
      const held = await topRow(page)
      const down = Math.min(step, (await place(page)).max - was)
      await page.keyboard.press('PageDown')
      await expect.poll(async () => {
        const now = await offsetOf(page, held!.id)
        return now === null ? -10_000 : now - held!.offset
      }, { timeout: 10_000 }).toBeLessThanOrEqual(-down + 150)
      const now = await offsetOf(page, held!.id)
      expect(now, 'the row the reader was on is still shown').not.toBeNull()
      expect(now! - held!.offset, 'PageDown moved the view no further than the reader').toBeGreaterThanOrEqual(-down - 150)
      was = (await place(page)).top
    }
    await pageDown()
    expect(was, 'PageDown brought the view down').toBeGreaterThan(0)
    await pageUp()

    // Step 5: another chat, then back. The chat reopens at its newest, and paging up works there too.
    const second = await echoBot(page, other)
    otherId = second.id
    await say(page, otherId, 'Say 1')
    await page.locator('.roster-row', { hasText: other }).click()
    await expect.poll(async () => (await place(page)).rows, { timeout: 20_000 }).toBeGreaterThan(0)
    await page.locator('.roster-row', { hasText: name }).click()
    await expect.poll(async () => (await place(page)).rows, { timeout: 20_000 }).toBe(40)

    // Step 6: PageUp again, after the round trip. Each press moves the view by a page, and PageDown still comes back.
    await focusRows(page)
    was = (await place(page)).top
    for (let press = 1; press <= 3; press++) await pageUp()
    await pageDown()
  } finally {
    if (otherId) await api(page, 'deleteBot', { botId: otherId }).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
  }
})
