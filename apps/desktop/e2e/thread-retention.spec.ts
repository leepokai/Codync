import { test, expect } from '@playwright/test'
import { api, appPage, attach, openPage } from './sandbox.ts'

type Hello = { threadKeepDays?: number }

test('Delete old threads keeps them forever by default, is picked on the computer card and follows another device', async () => {
  const { browser, page } = await attach()
  let was: number | undefined
  try {
    was = (await api<Hello>(page, 'hello', {})).threadKeepDays
    expect(was, 'the host keeps threads by a choice').toBeDefined()
    await api(page, 'setThreadKeepDays', { days: 0 })
    await openPage(page, 'Computers')
    const settings = appPage(page)
    const forever = settings.getByRole('button', { name: 'Keep forever' }).first()
    await expect(forever).toBeVisible()

    // Picked here: only the chosen value reaches the computer.
    await forever.click()
    await page.getByRole('menuitemradio', { name: 'After 90 days' }).click()
    await expect.poll(async () => (await api<Hello>(page, 'hello', {})).threadKeepDays).toBe(90)

    // Picked on another device (a phone, the terminal UI): the card follows the host's `threadKeepDays` event.
    await api(page, 'setThreadKeepDays', { days: 30 })
    await expect(settings.getByRole('button', { name: 'After 30 days' }).first()).toBeVisible()
    await openPage(page, 'Bots')
  } finally {
    await api(page, 'setThreadKeepDays', { days: was ?? 0 }).catch(() => {})
    await browser.close()
  }
})
