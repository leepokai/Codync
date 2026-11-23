import { test, expect, type Page } from '@playwright/test'
import { api, attach, openPage, steerBot } from './sandbox.ts'

/** Whether the sidebar shows `upper` above `lower`. */
async function above(page: Page, upper: string, lower: string): Promise<boolean> {
  const rows = await page.locator('.roster-row').allInnerTexts()
  const at = (name: string) => rows.findIndex((row) => row.includes(name))
  return at(upper) >= 0 && at(lower) >= 0 && at(upper) < at(lower)
}

/** "Sort by last active" in the Bots list's filter menu, opened. */
async function sortByLastActive(page: Page) {
  await openPage(page, 'Bots')
  await page.locator('.computer-filter').first().click()
  return page.getByRole('menuitemradio', { name: 'Sort by last active' })
}

/** The choice set to `on`, and the menu closed again. */
async function setAutoOrder(page: Page, on: boolean) {
  const choice = await sortByLastActive(page)
  if ((await choice.getAttribute('aria-checked')) !== String(on)) await choice.click()
  await expect(choice).toHaveAttribute('aria-checked', String(on))
  await page.keyboard.press('Escape')
}

test('the choice follows a toggle made on another device, without reconnecting', async () => {
  const { browser, page } = await attach()
  let was: boolean | undefined
  try {
    was = (await api<{ autoOrder?: boolean }>(page, 'hello', {})).autoOrder
    const choice = await sortByLastActive(page)
    for (const on of [!was, !!was]) {
      // Straight to the host, past this window's store: the way a phone or the terminal UI sets it.
      await api(page, 'setAutoOrder', { enabled: on })
      await expect(choice).toHaveAttribute('aria-checked', String(on))
    }
    await page.keyboard.press('Escape')
  } finally {
    if (was !== undefined) await api(page, 'setAutoOrder', { enabled: was }).catch(() => {})
    await browser.close()
  }
})

test('with "Sort by last active" on, a bot with a new message rises; off, the order stays', async () => {
  const { browser, page } = await attach()
  const stamp = Date.now()
  const [upper, lower] = [`Auto order top ${stamp}`, `Auto order low ${stamp}`]
  const ids: string[] = []
  let was: boolean | undefined
  try {
    was = (await api<{ autoOrder?: boolean }>(page, 'hello', {})).autoOrder
    for (const name of [upper, lower]) ids.push((await steerBot(page, name, 1)).id)
    await api(page, 'reorderBots', { ids })
    await expect.poll(() => above(page, upper, lower)).toBe(true)

    await setAutoOrder(page, true)
    await api(page, 'send', { botId: ids[1], text: 'hi' })
    await expect.poll(() => above(page, lower, upper)).toBe(true)

    await setAutoOrder(page, false)
    await api(page, 'send', { botId: ids[0], text: 'hi' })
    await page.waitForTimeout(3000) // The reply lands too, and neither moves the bot.
    expect(await above(page, lower, upper)).toBe(true)
  } finally {
    if (was !== undefined) await api(page, 'setAutoOrder', { enabled: was }).catch(() => {})
    for (const botId of ids) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
