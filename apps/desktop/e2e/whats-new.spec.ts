import { test, expect } from '@playwright/test'
import { announcements, due } from '../src/renderer/lib/whats-new.ts'
import { attach, setPref } from './sandbox.ts'

// What's new (docs/features/whats-new.md) after an update from 0.0.0: the window shows only what announcements.json
// says is due on this version, once. The file ships as [] (nothing shows); a temporary entry for the current version
// walks the window itself.
test("an update shows the due What's new once, and nothing when none is due", async () => {
  const { browser, page } = await attach()
  try {
    const version = await page.evaluate(() => window.codync.appVersion)
    const shown = due(announcements, '0.0.0', version, true)
    const sheet = page.getByRole('dialog').filter({ has: page.getByRole('heading', { name: "What's new" }) })
    const seen = () => page.evaluate(() => localStorage.getItem('whatsNewSeen'))

    await setPref(page, 'whatsNewSeen', '0.0.0')
    await page.reload()
    if (shown.length === 0) {
      await expect.poll(seen).toBe(JSON.stringify(version))
      await expect(sheet).toBeHidden()
      return
    }
    await expect(sheet).toBeVisible()
    for (const note of shown.flatMap((a) => a.notes)) await expect(sheet.getByText(note)).toBeVisible()
    await expect.poll(seen).toBe(JSON.stringify(version))
    await page.keyboard.press('Escape')
    await expect(sheet).toBeHidden()

    await page.reload()
    await expect(page.locator('#root')).not.toBeEmpty()
    await expect(sheet).toBeHidden()
  } finally {
    await page.evaluate(() => localStorage.setItem('whatsNewSeen', JSON.stringify(window.codync.appVersion)))
    await browser.close()
  }
})
