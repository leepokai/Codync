import { test, expect, type Page } from '@playwright/test'
import { attach, openGeneralSettings } from './sandbox.ts'

type Bridge = { codync: { notifications: { supported: boolean; enabled(): Promise<boolean>; setEnabled(on: boolean): Promise<boolean> } } }

/** The main process's saved setting, through the bridge the Settings switch uses. */
const saved = (page: Page) => page.evaluate(() => (window as unknown as Bridge).codync.notifications.enabled())
const save = (page: Page, on: boolean) => page.evaluate((on) => (window as unknown as Bridge).codync.notifications.setEnabled(on), on)

// Which alerts reach the main process is BotSync's (src/renderer/store/bot-sync.test.ts); the main process's
// bridge can't be spied on from the window, so a notification on screen is checked by hand.
test('system notifications start off and the switch keeps what it was set to', async () => {
  const { browser, page } = await attach()
  const toggle = page.getByRole('switch').filter({ hasText: 'Notifications' })
  try {
    test.skip(!(await page.evaluate(() => (window as unknown as Bridge).codync.notifications.supported)), 'This system shows no notifications.')
    await save(page, false) // A sandbox used before may have it on: off is what a new install has.

    await openGeneralSettings(page)
    await expect(toggle).toHaveAttribute('aria-checked', 'false')
    await toggle.click()
    await expect(toggle).toHaveAttribute('aria-checked', 'true')
    expect(await saved(page)).toBe(true)

    await page.reload()
    await openGeneralSettings(page)
    await expect(toggle).toHaveAttribute('aria-checked', 'true')
    await toggle.click()
    await expect(toggle).toHaveAttribute('aria-checked', 'false')
    expect(await saved(page)).toBe(false)
  } finally {
    await save(page, false).catch(() => {})
    await page.reload() // Closes Settings, which would cover the next spec.
    await browser.close()
  }
})
