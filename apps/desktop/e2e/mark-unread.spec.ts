import { test, expect, type Page } from '@playwright/test'
import { isBelow } from '../src/shared/compat.ts'
import { MARK_UNREAD_HOST } from '../src/renderer/store/read-marks.ts'
import { api, attach, steerBot, typeMessage } from './sandbox.ts'

const rowOf = (page: Page, name: string) => page.locator('.roster-row', { hasText: name })
const badge = (page: Page, name: string) => rowOf(page, name).locator('.unread-badge')

async function menu(page: Page, name: string) {
  await rowOf(page, name).click({ button: 'right' })
  const items = page.locator('.action-menu')
  await expect(items).toBeVisible()
  return items
}

async function pick(page: Page, name: string, title: string) {
  await (await menu(page, name)).getByText(title, { exact: true }).click()
}

/** The transcript's count of the scripted agent's replies. */
const replies = async (page: Page) => (await page.locator('.transcript').innerText()).match(/Done/g)?.length ?? 0

test.describe.serial('Mark as Unread', () => {
  const ids: string[] = []
  let browser: Awaited<ReturnType<typeof attach>>['browser']
  let page: Page
  let hostVersion: string

  /** A bot, open on screen, with one reply already read. */
  async function readChat(name: string, seconds = 0.1) {
    const { id } = await steerBot(page, name, seconds)
    ids.push(id)
    await rowOf(page, name).click()
    await api(page, 'send', { botId: id, text: 'one' })
    await expect(page.locator('.transcript')).toContainText('Done', { timeout: 15_000 })
    await page.waitForTimeout(300)
    return id
  }

  test.beforeAll(async () => {
    ({ browser, page } = await attach())
    hostVersion = (await api<{ version: string }>(page, 'hello', {})).version
  })
  // The roster offers it from this host version, which a sandbox of an older branch doesn't reach yet.
  test.beforeEach(() => test.skip(isBelow(hostVersion, MARK_UNREAD_HOST), `Needs a host from ${MARK_UNREAD_HOST}`))
  test.afterAll(async () => {
    for (const id of ids) await api(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  })

  test('it brings the count back and holds it on screen until opened again or marked read', async () => {
    const stamp = Date.now()
    const other = `Other ${stamp}`, name = `Held ${stamp}`
    const otherBot = await steerBot(page, other, 0.1)
    const { id } = await steerBot(page, name, 0.5, 'plain')
    ids.push(otherBot.id, id)
    await rowOf(page, other).click()
    await api(page, 'send', { botId: id, text: 'hello' })
    await api(page, 'send', { botId: id, text: 'and one more' })
    await expect(badge(page, name)).toHaveText('2', { timeout: 15_000 })
    await rowOf(page, name).click()
    await expect(badge(page, name)).toHaveCount(0)

    const read = await menu(page, name)
    await expect(read).toContainText('Mark as Unread')
    await expect(read).not.toContainText('Mark as Read')
    await read.getByText('Mark as Unread', { exact: true }).click()
    await expect(badge(page, name)).toHaveText('2')
    await page.locator('.transcript').click()
    await page.waitForTimeout(2000)
    await expect(badge(page, name)).toHaveText('2')

    const held = await menu(page, name)
    await expect(held).toContainText('Mark as Read')
    await expect(held).not.toContainText('Mark as Unread')
    await held.getByText('Mark as Read', { exact: true }).click()
    await expect(badge(page, name)).toHaveCount(0)

    // Away and back: opening reads it.
    await pick(page, name, 'Mark as Unread')
    await expect(badge(page, name)).toHaveText('2')
    await rowOf(page, other).click()
    await page.waitForTimeout(600)
    await expect(badge(page, name)).toHaveText('2')
    await rowOf(page, name).click()
    await expect(badge(page, name)).toHaveCount(0)

    // Marked from a chat that isn't on screen.
    await rowOf(page, other).click()
    await pick(page, name, 'Mark as Unread')
    await expect(badge(page, name)).toHaveText('2')
    await rowOf(page, name).click()
    await expect(badge(page, name)).toHaveCount(0)
  })

  test('with nothing unread before the last read, just the latest message', async () => {
    const name = `Latest ${Date.now()}`
    const id = await readChat(name)
    await api(page, 'send', { botId: id, text: 'two' })
    await expect.poll(() => replies(page), { timeout: 15_000 }).toBeGreaterThanOrEqual(2)
    await page.waitForTimeout(500)
    await expect(badge(page, name)).toHaveCount(0)
    await pick(page, name, 'Mark as Unread')
    await expect(badge(page, name)).toHaveText('1')
  })

  test('a reply landing while held adds to the count, on screen', async () => {
    const name = `Late ${Date.now()}`
    const id = await readChat(name, 3)
    await api(page, 'send', { botId: id, text: 'two' })
    await page.waitForTimeout(800)
    await pick(page, name, 'Mark as Unread')
    await expect(badge(page, name)).toHaveText('1')
    await expect(badge(page, name)).toHaveText('2', { timeout: 8000 })
    await page.waitForTimeout(1000)
    await expect(badge(page, name)).toHaveText('2')
  })

  test('sending from a held chat reads it, and the next reply on screen is read as usual', async () => {
    const name = `Send ${Date.now()}`
    await readChat(name, 0.5)
    await pick(page, name, 'Mark as Unread')
    await expect(badge(page, name)).toHaveText('1')
    await typeMessage(page, 'two')
    await expect(badge(page, name)).toHaveCount(0)
    await expect.poll(() => replies(page), { timeout: 15_000 }).toBeGreaterThanOrEqual(2)
    await page.waitForTimeout(800)
    await expect(badge(page, name)).toHaveCount(0)
  })

  test('marked on another device while on screen: held here too', async () => {
    const name = `Device ${Date.now()}`
    const id = await readChat(name)
    const { bot } = await api<{ bot: { keptUnread: boolean; unread: number } }>(page, 'markUnread', { botId: id })
    expect([bot.keptUnread, bot.unread]).toEqual([true, 1])
    await expect(badge(page, name)).toHaveText('1')
    await page.waitForTimeout(1500)
    await expect(badge(page, name)).toHaveText('1')
  })

  test('an empty chat has nothing to mark: no badge, and its first reply is read as usual', async () => {
    const name = `Empty ${Date.now()}`
    const { id } = await steerBot(page, name, 0.1)
    ids.push(id)
    await rowOf(page, name).click()
    await pick(page, name, 'Mark as Unread')
    await page.waitForTimeout(800)
    await expect(badge(page, name)).toHaveCount(0)
    await api(page, 'send', { botId: id, text: 'one' })
    await expect(page.locator('.transcript')).toContainText('Done', { timeout: 15_000 })
    await page.waitForTimeout(800)
    await expect(badge(page, name)).toHaveCount(0)
  })
})
