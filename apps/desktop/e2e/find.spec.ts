import { test, expect, type Browser, type Page } from '@playwright/test'
import { execFileSync } from 'node:child_process'
import { api, attach, echoBot, openGeneralSettings } from './sandbox.ts'

// ⌘F and ⌘G are menu accelerators, which keys sent over the DevTools protocol never reach: the specs pick the
// menu items through System Events instead (macOS). Every unpackaged sandbox app is a process named "Electron",
// so they pick the attached app by its process ID.
test.skip(process.platform !== 'darwin', 'drives the app menu through System Events')

type History = { entries: { kind: string; data: { text?: string } }[] }

/** Sends `text` and waits for the echo bot's answer, so every message is its own turn. */
async function say(page: Page, botId: string, text: string) {
  await api(page, 'send', { botId, text })
  await expect
    .poll(async () => (await api<History>(page, 'history', { botId })).entries.some((e) => e.kind !== 'user' && e.data.text?.endsWith(text)))
    .toBe(true)
}

/** The process ID of the app `browser` is attached to (its main process). */
async function appPid(browser: Browser) {
  const session = await browser.newBrowserCDPSession()
  const { processInfo } = await session.send('SystemInfo.getProcessInfo')
  await session.detach()
  const main = processInfo.find((p) => p.type === 'browser')
  if (!main) throw new Error('The attached app reported no main process.')
  return main.id
}

/** Picks an item of the View menu of the app with process ID `pid`, as its shortcut does. */
function viewMenu(pid: number, item: string) {
  execFileSync('osascript', ['-e', `tell application "System Events" to tell (first process whose unix id is ${pid}) to click menu item "${item}" of menu "View" of menu bar 1`])
}

/** ⌘F: View > Find in Conversation. */
async function openFind(page: Page, pid: number) {
  viewMenu(pid, 'Find in Conversation')
  await expect(page.getByRole('search')).toBeVisible()
}

/** The find bar's field and count. */
const field = (page: Page) => page.getByRole('search').getByRole('textbox')
const count = (page: Page) => page.getByRole('search').locator('.find-count')

/** The current match: its text, the message it is in, and whether it shows in every box that scrolls around it. */
function current(page: Page) {
  return page.evaluate(() => {
    const highlight = CSS.highlights.get('find-active') as unknown as Set<Range> | undefined
    const range = highlight ? [...highlight][0] : undefined
    if (!range) return null
    const rect = range.getBoundingClientRect()
    let inView = true
    let sideways = 0
    for (let el = range.startContainer.parentElement; el; el = el.parentElement) {
      const style = getComputedStyle(el)
      const box = el.getBoundingClientRect()
      if (style.overflowX !== 'visible' && el.scrollWidth > el.clientWidth) {
        sideways += el.scrollLeft
        inView &&= rect.left >= box.left && rect.right <= box.left + el.clientWidth
      }
      if (style.overflowY !== 'visible' && el.scrollHeight > el.clientHeight) inView &&= rect.top >= box.top && rect.bottom <= box.top + el.clientHeight
    }
    return { text: range.toString(), message: range.startContainer.parentElement?.closest('.message-body, .bubble')?.textContent ?? '', inView, sideways }
  })
}

const TABLE = 'A table:\n\n| Name | Status |\n|---|---|\n| Alice | Ready |\n| Bob | Busy |'
const WIDE = 'A wide line:\n\n```\n' + 'filler '.repeat(160) + 'farthest\n```'

test('find in a conversation keeps to the messages, steps through matches and keeps its place', async () => {
  test.setTimeout(120_000) // About thirty turns go by first.
  const { browser, page } = await attach()
  const name = `Find e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const pid = await appPid(browser)
    const bot = await echoBot(page, name)
    botId = bot.id
    // One match far back, past the first page of the chat, then enough to push it there.
    await say(page, botId, 'needle one')
    for (let i = 1; i <= 22; i++) await say(page, botId, `Filler ${i}`)
    await say(page, botId, TABLE)
    await say(page, botId, WIDE)
    await say(page, botId, 'needle two')
    await say(page, botId, 'needle three')

    await page.locator('.roster-row', { hasText: name }).click()
    await expect(page.locator('.load-earlier')).toBeVisible()
    await openFind(page, pid)

    // The transcript's own controls aren't the conversation.
    await field(page).fill('earlier')
    await expect(count(page)).toHaveText('No results')

    // Table cells are kept apart: no match runs from one cell into the next.
    await field(page).fill('StatusAlice')
    await expect(count(page)).toHaveText('No results')
    await field(page).fill('AliceReady')
    await expect(count(page)).toHaveText('No results')
    await field(page).fill('Ready')
    await expect(count(page)).toHaveText('1/2')

    // A match past the right edge of a code line is scrolled into sight.
    await field(page).fill('farthest')
    await expect(count(page)).toHaveText('1/2')
    await field(page).press('Enter')
    await expect(count(page)).toHaveText('2/2')
    const wide = await current(page)
    expect(wide?.text).toBe('farthest')
    expect(wide?.inView).toBe(true)
    expect(wide?.sideways).toBeGreaterThan(0)

    // Each message echoes back, so every needle is there twice; "needle one" isn't loaded yet.
    await field(page).fill('needle')
    await expect(count(page)).toHaveText('1/4')
    await field(page).press('Enter')
    await field(page).press('Enter')
    await field(page).press('Shift+Enter')
    await expect(count(page)).toHaveText('2/4')
    viewMenu(pid, 'Find Next')
    await expect(count(page)).toHaveText('3/4')
    viewMenu(pid, 'Find Previous')
    await expect(count(page)).toHaveText('2/4')
    await page.getByRole('search').getByRole('button', { name: 'Next match' }).click()
    await expect(count(page)).toHaveText('3/4')
    expect((await current(page))?.message).toContain('needle three')

    // Earlier messages loading above (reading up to the top loads them) keep the same match current.
    await page.locator('.transcript').evaluate((el) => el.scrollTo({ top: 0 }))
    await expect(count(page)).toHaveText('5/6')
    expect((await current(page))?.message).toContain('needle three')

    // A menu or sheet opened over the bar takes Escape first.
    await page.locator('.sidebar-account').click()
    await expect(page.locator('.account-panel')).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(page.locator('.account-panel')).toHaveCount(0)
    await expect(page.getByRole('search')).toBeVisible()
    await openGeneralSettings(page)
    await expect(page.getByRole('dialog')).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(page.getByRole('search')).toBeVisible()

    // Escape closes the bar from anywhere in the window, not only its field.
    await page.locator('.composer textarea').click()
    await page.keyboard.press('Escape')
    await expect(page.getByRole('search')).toHaveCount(0)
    expect(await page.evaluate(() => CSS.highlights.has('find-match'))).toBe(false)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test("an empty chat's hints aren't searched", async () => {
  const { browser, page } = await attach()
  const name = `Find empty e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const pid = await appPid(browser)
    botId = (await echoBot(page, name)).id
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(page.locator('.intro-card')).toContainText('notification')
    await openFind(page, pid)
    await field(page).fill('notification')
    await expect(count(page)).toHaveText('No results')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
