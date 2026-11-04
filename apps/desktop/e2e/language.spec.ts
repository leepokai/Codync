import { test, expect } from '@playwright/test'
import { attach } from './sandbox.ts'

test('choosing Italiano translates the interface', async () => {
  const { browser, page } = await attach()
  try {
    // A sandbox keeps its window's settings between runs, and by default follows the computer's language: start in English.
    await page.evaluate(() => localStorage.setItem('appLanguage', JSON.stringify('en')))
    await page.reload()
    await page.locator('.sidebar-account').click()
    await page.locator('.account-panel').getByText('Settings', { exact: true }).click()
    await page.locator('.choice-picker[title="English"]').click()
    await page.getByText('Italiano', { exact: true }).click()
    await expect(page.locator('.choice-picker[title="Italiano"]')).toBeVisible()
    await expect(page.getByText('Lingua app', { exact: true })).toBeVisible()
    await expect(page.getByText('App language', { exact: true })).toHaveCount(0)
  } finally {
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await page.reload()
    await browser.close()
  }
})
