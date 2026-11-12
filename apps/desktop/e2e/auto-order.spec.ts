import { test, expect, type Page } from '@playwright/test'
import { api, attach, openGeneralSettings, steerBot } from './sandbox.ts'

/** Whether the sidebar shows `upper` above `lower`. */
async function above(page: Page, upper: string, lower: string): Promise<boolean> {
  const rows = await page.locator('.roster-row').allInnerTexts()
  const at = (name: string) => rows.findIndex((row) => row.includes(name))
  return at(upper) >= 0 && at(lower) >= 0 && at(upper) < at(lower)
}

/** Settings › Computers, this computer's "Move active bots to the top" switch, set to `on`. */
async function setAutoOrder(page: Page, on: boolean) {
  await openGeneralSettings(page)
  const dialog = page.getByRole('dialog')
  await dialog.getByRole('button', { name: 'Computers', exact: true }).click()
  const toggle = dialog.getByRole('switch', { name: 'Move active bots to the top' }).first()
  if ((await toggle.getAttribute('aria-checked')) !== String(on)) await toggle.click()
  await expect(toggle).toHaveAttribute('aria-checked', String(on))
  await page.keyboard.press('Escape')
}

test('the switch follows a toggle made on another device, without reconnecting', async () => {
  const { browser, page } = await attach()
  let was: boolean | undefined
  try {
    was = (await api<{ autoOrder?: boolean }>(page, 'hello', {})).autoOrder
    await openGeneralSettings(page)
    const dialog = page.getByRole('dialog')
    await dialog.getByRole('button', { name: 'Computers', exact: true }).click()
    const toggle = dialog.getByRole('switch', { name: 'Move active bots to the top' }).first()
    for (const on of [!was, !!was]) {
      // Straight to the host, past this window's store: the way a phone or the terminal UI sets it.
      await api(page, 'setAutoOrder', { enabled: on })
      await expect(toggle).toHaveAttribute('aria-checked', String(on))
    }
    await page.keyboard.press('Escape')
  } finally {
    if (was !== undefined) await api(page, 'setAutoOrder', { enabled: was }).catch(() => {})
    await browser.close()
  }
})

test('with "Move active bots to the top" on, a bot with a new message rises; off, the order stays', async () => {
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
