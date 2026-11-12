import { test, expect, type Page } from '@playwright/test'
import { join, resolve } from 'node:path'
import { api, attach, freezeHost } from './sandbox.ts'

// A scripted agent that answers `Message <n>` with `Reply <n>` after the tool calls the message names
// (host/tests/fixtures/history_agent.py). Playwright loads specs as CommonJS here, so `__dirname` is defined.
const agent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/history_agent.py')

type Entry = { id: string; botId: string; seq: number; kind: string; data: { text?: string; final?: boolean } }
type Cache = { stamp: string; entries: Entry[] }

const GONE = 'Taken off the computer'

async function sendAndWait(page: Page, botId: string, n: number, text: string) {
  await api(page, 'send', { botId, text })
  await expect.poll(async () => {
    const { entries } = await api<{ entries: Entry[] }>(page, 'history', { botId, limit: 5 })
    return entries.some((e) => e.data.final && e.data.text === `Reply ${n}`)
  }, { timeout: 20_000 }).toBe(true)
}

/** The saved copy of the conversation holding `botId`'s chat, once it is saved whole (a stamp, the last reply). */
const savedChat = (page: Page, botId: string) => page.evaluate((botId) => {
  for (const key of Object.keys(localStorage).filter((k) => k.startsWith('codync-mirror-'))) {
    const cache = JSON.parse(localStorage.getItem(key)!) as Cache
    if (cache.stamp && cache.entries.some((e) => e.botId === botId && e.data.text === 'Reply 2')) return key
  }
  return null
}, botId)

test('a chat keeps its messages while a new version fetches it again, then holds what the computer has', async () => {
  test.skip(process.platform === 'win32', 'Windows has no signal that freezes a process.')
  const { browser, page } = await attach()
  const name = `Keep chat e2e ${Date.now()}`
  const bubble = (text: string) => page.locator('.bubble', { hasText: text })
  let botId: string | undefined
  let thaw: (() => void) | undefined
  try {
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: `python3 -X utf8 -u '${agent}'`, connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    // 260 tool rows put the first message below the newest 200 a catch-up from the start brings.
    await sendAndWait(page, bot.id, 1, 'Message 1 opens with 260 tool calls')
    await sendAndWait(page, bot.id, 2, 'Message 2')
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(bubble('Reply 2')).toBeVisible()
    await expect.poll(() => savedChat(page, bot.id)).not.toBeNull()
    const key = (await savedChat(page, bot.id))!

    // The copy another version saved, with a message the computer has since lost, read while the computer is silent.
    thaw = freezeHost()
    await page.evaluate(([key, botId, text]) => {
      const cache = JSON.parse(localStorage.getItem(key)!) as Cache
      const last = cache.entries.filter((e) => e.botId === botId).at(-1)!
      cache.entries.push({ ...last, id: 'gone-e2e', seq: last.seq + 1, data: { ...last.data, text } })
      localStorage.setItem(key, JSON.stringify({ ...cache, stamp: '0.0.1/0.0.1' }))
    }, [key, bot.id, GONE] as const)
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(bubble('Reply 2')).toBeVisible()
    await expect(bubble(GONE)).toBeVisible()

    // The computer answers: everything comes again underneath, and the chat never empties meanwhile.
    await page.evaluate(() => {
      const shown = () => [...document.querySelectorAll('.bubble')].some((b) => b.textContent?.includes('Reply 2'))
      const state = window as unknown as { chatEmptied: boolean }
      state.chatEmptied = false
      new MutationObserver(() => { if (!shown()) state.chatEmptied = true }).observe(document.body, { childList: true, subtree: true })
    })
    thaw()
    thaw = undefined
    await expect(bubble(GONE)).toHaveCount(0, { timeout: 20_000 })
    await expect(bubble('Reply 2')).toBeVisible()
    await expect(bubble('Message 2')).toBeVisible()
    expect(await page.evaluate(() => (window as unknown as { chatEmptied: boolean }).chatEmptied)).toBe(false)
    await expect.poll(() => page.evaluate((key) => (JSON.parse(localStorage.getItem(key)!) as Cache).stamp, key)).not.toBe('0.0.1/0.0.1')

    // Earlier messages still page in from the computer.
    const earlier = page.locator('.load-earlier')
    const first = bubble('Message 1 opens with 260 tool calls')
    await expect(async () => {
      if (!(await first.count())) await earlier.click()
      await expect(first).toHaveCount(1, { timeout: 1_000 })
    }).toPass({ timeout: 20_000 })
  } finally {
    thaw?.()
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
