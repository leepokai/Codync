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
    // the bottom after every render. The anchor taken there used to outlive the render that took it — only the
    // reading-history branch cleared it — and the next render after the reader left the bottom applied it,
    // overriding the top the reader had chosen. Dispatched rather than clicked: the button sits above the rows,
    // and a click would scroll the transcript first, which is not what this pins.
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
