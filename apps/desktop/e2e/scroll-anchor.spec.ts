import { test, expect, type Page } from '@playwright/test'
import { api, attach, echoBot } from './sandbox.ts'

// `mode_agent.py` answers each message with its text, so a chat of a few dozen turns is quick to build.
type Entry = { kind: string; data: { text?: string } }

/** Sends `text` and waits for the echo bot's answer. */
async function say(page: Page, botId: string, text: string) {
  await api(page, 'send', { botId, text })
  await waitForAnswer(page, botId, text)
}

/** Waits until the bot's answer to `text` is in the computer's history. */
const waitForAnswer = (page: Page, botId: string, text: string) => expect.poll(
  async () => (await api<{ entries: Entry[] }>(page, 'history', { botId, limit: 6 })).entries.some((e) => e.kind !== 'user' && e.data.text?.includes(text)),
  { timeout: 20_000 },
).toBe(true)

/** Where the transcript sits, how far it can scroll, and how many rows it shows. */
const place = (page: Page) => page.evaluate(() => {
  const el = document.querySelector<HTMLElement>('.transcript')!
  return { top: Math.round(el.scrollTop), max: Math.round(el.scrollHeight - el.clientHeight), rows: el.querySelectorAll('[data-entry-id]').length }
})

/**
 * How many rows the chat has loaded, read from the component itself: the DOM shows a window of them, and only a page
 * fetched from the computer makes that number grow.
 */
const loadedRows = (page: Page) => page.evaluate(() => {
  const el = document.querySelector<HTMLElement>('.transcript')
  if (!el) return -1
  const key = Object.keys(el).find((k) => k.startsWith('__reactFiber$'))
  let fiber = key ? (el[key as keyof HTMLElement] as unknown as { memoizedProps?: { items?: unknown[] }; return: unknown }) : null
  for (let up = 0; fiber && up < 40; up++) {
    const props = fiber.memoizedProps as { loadOlder?: unknown; items?: unknown[] } | undefined
    if (props && typeof props.loadOlder === 'function') return props.items?.length ?? 0
    fiber = fiber.return as typeof fiber
  }
  return -1
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

/** A window that came back after a restart, with whatever it put up on the way. */
const settle = async (page: Page) => {
  for (let i = 0; i < 40; i++) {
    const s = await page.evaluate(() => ({ c: !!document.querySelector('.welcome-continue'), nav: document.querySelectorAll('.nav-rail button').length, w: !!document.querySelector('.welcome') }))
    if (s.c) { await page.evaluate(() => document.querySelector<HTMLElement>('.welcome-continue')!.click()); await page.waitForTimeout(2500); continue }
    if (s.nav > 0 && !s.w) return
    await page.waitForTimeout(500)
  }
}

test('a page of earlier rows asked for at the newest does not move the reader afterwards', async () => {
  test.setTimeout(180_000) // Twenty-six turns go by first.
  const { page } = await attach()
  // A sandbox nobody has opened yet is still on its welcome screen, where no `.roster-row` exists: without this the
  // test would time out rather than fail on what it is about. A sandbox already signed in returns at once.
  await settle(page)
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

test('a message arriving while the reader is inside the first page does not move them', async () => {
  test.setTimeout(180_000) // Twenty-six turns go by first.
  const { page } = await attach()
  await settle(page)
  const name = `Scroll anchor arrival ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    for (let i = 1; i <= 26; i++) await say(page, bot.id, `Say ${i}`)
    await page.locator('.roster-row', { hasText: name }).click()
    await expect.poll(async () => (await place(page)).rows, { timeout: 20_000 }).toBe(40)

    // The reader scrolls up inside the newest 40 rows, but nowhere near the 200 px threshold that asks for earlier
    // rows. No page has been taken, so the window is still "the newest 40".
    const room = await place(page)
    expect(room.max, 'the first page has room to read above the newest').toBeGreaterThan(700)
    await scrollTo(page, 500)
    await expect.poll(async () => (await place(page)).top, { timeout: 10_000 }).toBeGreaterThanOrEqual(450)
    const held = await topRow(page)
    expect(held, 'a row was at the top edge before the arrival').not.toBeNull()
    const before = await place(page)

    // A bot message arriving on its own, as a background result does. It has no user row in front of it and brings
    // no page with it; the newest-40 start must not move and take a row from above the reader.
    await api(page, 'chatCall', { botId, name: 'send_message', arguments: { text: 'A message while reading' } })
    await expect(page.locator('.message.agent', { hasText: 'A message while reading' })).toBeVisible({ timeout: 20_000 })
    await expect.poll(async () => {
      const now = await offsetOf(page, held!.id)
      return now === null ? 10_000 : Math.abs(now - held!.offset)
    }, { timeout: 10_000 }).toBeLessThanOrEqual(8)
    expect(Math.abs((await place(page)).top - before.top), 'the arrival moved the view').toBeLessThanOrEqual(8)
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
  await settle(page)
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
     * view — less only when they were nearer the top than a page — and no further, whichever page of earlier rows
     * arrives on the way. A view thrown to its top takes the row right off the screen, or leaves it a page or more
     * away.
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

/** The view's own height, which is the room the rows have to show in. */
const viewHeight = (page: Page) => page.evaluate(() => Math.round(document.querySelector<HTMLElement>('.transcript')!.clientHeight))

/** Resizes the app's window, as a reader dragging its corner or a docked laptop does. Returns the size to put back. */
const resizeWindow = async (page: Page, width: number, height: number) => {
  const was = await page.evaluate(() => ({ w: window.outerWidth, h: window.outerHeight }))
  await page.evaluate(([w, h]) => window.resizeTo(w, h), [width, height] as const)
  await page.waitForTimeout(600)
  return was
}

/**
 * The view shrinking under a reader in the history — the other half of the report.
 *
 * A shorter view is not an arrival of rows: nothing above the reader moved, so nothing may be paid for and the view
 * may not be re-placed. What must not happen is what the distance-from-the-rows'-end place did: it read the new, taller
 * distance and put the view somewhere else entirely, and once the rows were shorter than that distance the browser
 * clamped the view to 0 — the top of the oldest rows held — with every later render clamping it again.
 */
test('a view that shrinks under a reader in the history keeps their row where it was', async () => {
  test.setTimeout(180_000)
  const { page } = await attach()
  await settle(page)
  const name = `Scroll anchor resize ${Date.now()}`
  let botId: string | undefined
  let was: { w: number; h: number } | null = null
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    for (let i = 1; i <= 70; i++) await say(page, bot.id, `Say ${i} · padding text for a realistic bubble`)
    await page.locator('.roster-row', { hasText: name }).click()
    await expect.poll(async () => (await place(page)).rows, { timeout: 30_000 }).toBe(40)

    // The reader scrolls into the history, well away from both ends, so the view is theirs and not the bottom's. A
    // page of earlier rows is brought in first, in memory: the place kept here is a reader's, not a follower's.
    await focusRows(page)
    await scrollTo(page, 0)
    await expect.poll(async () => (await place(page)).rows, { timeout: 20_000 }).toBeGreaterThan(60)
    const before = await place(page)
    const middle = Math.round(before.max * 0.55)
    expect(middle, 'the rows can scroll far enough to be in the middle').toBeGreaterThan(1000)
    await scrollTo(page, middle)
    await expect.poll(async () => (await place(page)).top, { timeout: 10_000 }).toBeGreaterThan(middle - 40)
    expect((await place(page)).top, 'the view stayed where the reader put it').toBeLessThanOrEqual(middle + 40)
    const held = await topRow(page)
    expect(held, 'a row was at the top edge before the resize').not.toBeNull()

    // Step 1: the window shrinks, as a docked laptop or a dragged corner does. The rows do not move; the view's room
    // does. The row the reader was on stays exactly where it was.
    const tall = await viewHeight(page)
    was = await resizeWindow(page, 1000, 560)
    const short = await viewHeight(page)
    expect(short, 'the view got shorter').toBeLessThan(tall)
    const shrunk = await place(page)
    expect(shrunk.top, 'the shorter view did not throw the rows to their top').toBeGreaterThan(200)
    const afterShrink = await offsetOf(page, held!.id)
    expect(afterShrink, 'the row the reader was on is still shown').not.toBeNull()
    expect(Math.abs(afterShrink! - held!.offset), 'the shrink moved the reader off their row').toBeLessThanOrEqual(60)

    // Step 2: and the reader can still page up and down by a page from there.
    await focusRows(page)
    const step = await pageStep(page)
    const up = Math.min(step, (await place(page)).top)
    await page.keyboard.press('PageUp')
    await expect.poll(async () => {
      const now = await offsetOf(page, held!.id)
      return now === null ? -10_000 : now - afterShrink!
    }, { timeout: 10_000 }).toBeGreaterThanOrEqual(up - 200)
    const now = await offsetOf(page, held!.id)
    expect(now, 'the row the reader was on is still shown after paging').not.toBeNull()

    // Step 3: the window comes back, as the laptop is undocked. That must not move the reader either.
    await page.evaluate(([w, h]) => window.resizeTo(w, h), [was.w, was.h] as [number, number])
    await page.waitForTimeout(600)
    await expect.poll(async () => await viewHeight(page), { timeout: 10_000 }).toBe(tall)
    const restored = await offsetOf(page, held!.id)
    expect(restored, 'the row the reader was on is still shown after the window came back').not.toBeNull()
    expect(Math.abs(restored! - now!), 'the window coming back moved the reader off their row').toBeLessThanOrEqual(60)
  } finally {
    if (was) await page.evaluate(([w, h]) => window.resizeTo(w, h), [was.w, was.h] as [number, number]).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
  }
})

/**
 * The page that has to be FETCHED — the one the reported jump came from.
 *
 * Paging up inside the rows the app already holds is all in memory: the press, the "loading" state and the new
 * first row land in one render, so no place kept can be spent on the wrong one. At the oldest rows held, the page
 * comes from the computer over a round trip, so the app commits at least once while it is on its way. A window
 * filed under a row's id could not survive that: the row it was filed under is often a date separator that stops
 * existing the moment the older rows arrive, so the window fell back to the newest page — 261 rows away in the
 * measured case — and every later press walked into the same trap.
 *
 * So this one starts a chat with more turns than the app holds when it launches, restarts the window, and pages up
 * to the fetch. Every press must leave the row the reader was on in front of them, moved by about their own page;
 * the press that makes the loaded rows grow is the one this exists for.
 */
test('the page fetched from the computer keeps the row the reader was on', async () => {
  test.setTimeout(420_000) // A hundred and thirty turns go by first, then the paging.
  const { page } = await attach()
  const name = `Scroll anchor fetch ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    // More entries than the app holds for a chat it did not open: 130 turns are 261 entries, and after a restart the
    // app holds the newest 200 of them, so paging up has to ask the computer for the rest. Each turn is waited for
    // before the next one is sent: a burst of sends outruns the bot and the computer drops turns, which leaves a chat
    // too short to page through (and one whose older rows the app already holds, so no fetch is ever made).
    for (let i = 1; i <= 130; i++) await say(page, bot.id, `Say ${i} · padding text for a realistic bubble`)

    // Let the app write its cache before the restart, as a launch after a quiet chat finds it: the cache keeps the
    // newest 200 entries of each chat, so that is what the restarted window holds, and the rest has to be fetched.
    // Restarting right after the last turn instead leaves the cache saved mid-seed, and the catch-up then tops it up
    // to the whole chat — nothing left to fetch.
    await page.waitForTimeout(5000)

    // Restart the window, as an app launch does, so it holds only the newest entries of each chat.
    await page.reload()
    await settle(page)
    await page.locator('.roster-row', { hasText: name }).click()
    await expect.poll(async () => (await place(page)).rows, { timeout: 30_000 }).toBe(40)

    // Step 1: the large paste, as in the report: the box grows and the rows' bottom padding with it.
    const boxBefore = await composerHeight(page)
    await page.locator('.composer:not(.thread-chat .composer) textarea').fill(Array.from({ length: 60 }, (_, i) => `line ${i + 1} of a long pasted message`).join('\n'))
    await expect.poll(async () => composerHeight(page), { timeout: 10_000 }).toBeGreaterThan(boxBefore + 40)
    expect((await place(page)).top, 'the paste did not throw the rows to their top').toBeGreaterThan(2)

    // Step 2: paging up, press by press, to the oldest rows held and then past them.
    await focusRows(page)
    const step = await pageStep(page)
    const loadedBefore = await loadedRows(page)
    // The probe returns -1 when it cannot find the component, and `toBeLessThan(300)` would accept that: a -1 that
    // later read as a real number would look like growth on the first press. Read a real count first.
    expect(loadedBefore, 'the row count is a real reading, not the probe\'s -1').toBeGreaterThan(0)
    expect(loadedBefore, 'the window holds a window of the chat, not all of it').toBeLessThan(300)
    let grewOnPress = 0
    for (let press = 1; press <= 30 && grewOnPress === 0; press++) {
      const held = await topRow(page)
      expect(held, 'a row was at the top edge before the press').not.toBeNull()
      const before = await place(page)
      const up = Math.min(step, before.top)
      await page.keyboard.press('PageUp')
      await expect.poll(async () => {
        const now = await offsetOf(page, held!.id)
        return now === null ? -10_000 : now - held!.offset
      }, { timeout: 10_000 }).toBeGreaterThanOrEqual(up - 150)
      const now = await offsetOf(page, held!.id)
      // The sharpest statement of the bug: the row the reader was reading must still be on screen after the press.
      expect(now, `the row the reader was on is still shown after press ${press}`).not.toBeNull()
      expect(now! - held!.offset, `press ${press} moved the view no further than the reader`).toBeLessThanOrEqual(up + 150)
      if ((await loadedRows(page)) > loadedBefore) grewOnPress = press
    }
    expect(grewOnPress, 'a page was fetched from the computer while paging up').toBeGreaterThan(0)

    // And the view is still in the history, not thrown back to the newest page: the reader can go on up.
    await page.keyboard.press('PageUp')
    await expect.poll(async () => (await place(page)).top, { timeout: 10_000 }).toBeGreaterThan(0)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
  }
})

/**
 * Jump to latest wins over a history request already in flight. When the late answer arrives it belongs to the window
 * the reader abandoned; installing it again would render an old explicit page beside the follower, and each arrival
 * would then grow the transcript beyond the newest page.
 */
test('a late history answer does not undo Jump to latest', async () => {
  test.setTimeout(300_000) // Twenty-six turns go by first, then the held request.
  const { page } = await attach()
  await settle(page)
  const name = `Scroll anchor stale ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    for (let i = 1; i <= 26; i++) await say(page, bot.id, `Say ${i}`)
    await page.locator('.roster-row', { hasText: name }).click()
    await expect.poll(async () => (await place(page)).rows, { timeout: 20_000 }).toBe(40)
    // Leave the bottom, so this is a reader in the history: Jump to latest is present and a late page has a window to
    // restore if the code lets it.
    await scrollTo(page, 300)
    await expect.poll(async () => (await place(page)).top, { timeout: 10_000 }).toBeGreaterThanOrEqual(250)
    await expect(page.getByRole('button', { name: 'Jump to latest' })).toBeVisible()

    // Hold the next loadOlder before it reaches the store. A loaded page still reports history as available until a
    // fetch comes back short, so the first Load earlier at the oldest row is the request held here. The store instance
    // is reached through the chat's fiber: `window.codync` is frozen by the preload bridge and cannot be wrapped.
    await page.evaluate(() => {
      const el = document.querySelector<HTMLElement>('.transcript')!
      const key = Object.keys(el).find((k) => k.startsWith('__reactFiber$'))!
      let fiber = (el as unknown as Record<string, { memoizedProps?: { value?: { loadOlder?: (...args: unknown[]) => Promise<unknown> } }; return: unknown }>)[key]
      let store: { loadOlder?: (...args: unknown[]) => Promise<unknown> } | undefined
      for (let up = 0; fiber && up < 40; up++) {
        const value = fiber.memoizedProps?.value
        if (value && typeof value.loadOlder === 'function' && value.constructor?.name === 'BotStore') { store = value; break }
        fiber = fiber.return as typeof fiber
      }
      if (!store?.loadOlder) throw new Error('the chat store was not found')
      const original = store.loadOlder.bind(store)
      const w = window as unknown as {
        __scrollGate?: { held: boolean; used: boolean; release?: () => void; original?: () => void }
      }
      const gate = { held: false, used: false } as NonNullable<typeof w.__scrollGate>
      w.__scrollGate = gate
      store.loadOlder = async (...args: unknown[]) => {
        if (!gate.used) {
          gate.used = true
          gate.held = true
          await new Promise<void>((release) => { gate.release = release })
        }
        return original(...args)
      }
      gate.original = () => { store!.loadOlder = original }
    })
    // The newest window starts below the top of the loaded rows; the first press opens those locally, the next asks the
    // computer. Dispatched rather than clicked for the same reason test 1 does: the button sits above the rows.
    for (let press = 0; press < 4; press++) {
      if (await page.evaluate(() => (window as unknown as { __scrollGate?: { held: boolean } }).__scrollGate?.held)) break
      await page.locator('.load-earlier').dispatchEvent('click')
      await page.waitForTimeout(150)
    }
    await expect.poll(
      async () => page.evaluate(() => (window as unknown as { __scrollGate?: { held: boolean } }).__scrollGate?.held),
      { message: 'the history request was held before it answered', timeout: 10_000 },
    ).toBe(true)
    await expect(page.getByRole('button', { name: 'Jump to latest' })).toBeVisible()

    await page.getByRole('button', { name: 'Jump to latest' }).click()
    await page.evaluate(() => (window as unknown as { __scrollGate?: { release?: () => void } }).__scrollGate?.release?.())
    await expect.poll(async () => (await place(page)).rows, { timeout: 20_000 }).toBe(40)
    await api(page, 'chatCall', { botId: botId!, name: 'send_message', arguments: { text: 'After the stale history response' } })
    await expect(page.locator('.message.agent', { hasText: 'After the stale history response' })).toBeVisible({ timeout: 20_000 })
    await expect.poll(async () => (await place(page)).rows, { message: 'the follower still renders only the newest page', timeout: 10_000 }).toBe(40)
    await expect.poll(async () => {
      const now = await place(page)
      return now.max - now.top
    }, { timeout: 10_000 }).toBeLessThan(8)
  } finally {
    // Let a held request finish even when the assertion above failed, then put the store's method back.
    await page.evaluate(() => {
      const w = window as unknown as { __scrollGate?: { release?: () => void; original?: () => void } }
      w.__scrollGate?.release?.()
      w.__scrollGate?.original?.()
    }).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
  }
})
