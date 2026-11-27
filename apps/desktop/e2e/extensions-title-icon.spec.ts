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
    // Same symbol as the rail entry: its mask on macOS, its fallback drawing elsewhere.
    const shape = (l: typeof icon) => l.evaluate((e) => (e as HTMLElement).style.maskImage || e.innerHTML)
    const drawn = await shape(icon)
    expect(drawn).not.toBe('')
    expect(drawn).toBe(await shape(rail.locator('svg, span[aria-hidden]').first()))
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
