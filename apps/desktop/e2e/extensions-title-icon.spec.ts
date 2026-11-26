import { test, expect } from '@playwright/test'
import { appPage, attach, openPage } from './sandbox.ts'

// The Extensions title wears the rail's own icon, and the computer capsule still sits beside it.
test('the Extensions title leads with its rail icon, the computer capsule beside it', async () => {
  const { browser, page } = await attach()
  try {
    await openPage(page, 'Extensions')
    const titles = appPage(page).locator('.market-heading-titles')
    const [icon, title, capsule] = [titles.locator('> *').nth(0), titles.locator('> *').nth(1), titles.locator('> *').nth(2)]
    await expect(title).toHaveText('Extensions')
    const rail = page.locator('.nav-rail').getByRole('button', { name: 'Extensions', exact: true })
    // Same symbol as the rail entry.
    const symbol = async (l: typeof icon) => (await l.evaluate((e) => e.outerHTML)).match(/square\.grid\.2x2/)?.[0] ?? null
    expect(await symbol(icon)).toBe('square.grid.2x2')
    expect(await rail.evaluate((e) => e.outerHTML)).toContain('square.grid.2x2')
    for (const scheme of ['light', 'dark'] as const) {
      await page.emulateMedia({ colorScheme: scheme })
      const [i, t, c] = await Promise.all([icon, title, capsule].map(async (l) => (await l.boundingBox())!))
      expect(i.x + i.width).toBeLessThanOrEqual(t.x + 1)
      expect(t.x + t.width).toBeLessThanOrEqual(c.x + 1)
      for (const b of [i, c]) expect(Math.abs(b.y + b.height / 2 - (t.y + t.height / 2))).toBeLessThan(4)
      await expect(capsule).toHaveClass(/choice-picker|market-computer/)
      await appPage(page).locator('.market-heading').screenshot({ path: process.env.SHOT_DIR ? `${process.env.SHOT_DIR}/extensions-heading-${scheme}.png` : undefined })
    }
  } finally {
    await page.emulateMedia({ colorScheme: null })
    await openPage(page, 'Bots').catch(() => {})
    await browser.close()
  }
})
