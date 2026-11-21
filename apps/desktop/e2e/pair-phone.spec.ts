import { test, expect } from '@playwright/test'
import { api, attach, openGeneralSettings } from './sandbox.ts'

type Hello = { name: string }

test('Settings › Computers pairs any phone: "Pair phone" opens "Pair phone with <name>", with no iPhone-only copy', async () => {
  const { browser, page } = await attach()
  try {
    const { name } = await api<Hello>(page, 'hello', {})
    await openGeneralSettings(page)
    const settings = page.getByRole('dialog')
    await settings.getByRole('button', { name: 'Computers', exact: true }).click()
    await expect(settings.getByRole('button', { name: 'Pair iPhone' })).toHaveCount(0)
    await settings.getByRole('button', { name: 'Pair phone', exact: true }).first().click()

    const sheet = page.getByRole('dialog').filter({ hasText: `Pair phone with ${name}` })
    await expect(sheet).toBeVisible()
    await expect(sheet.getByText(/Scan with the Codync app or your phone's camera\./)).toBeVisible()
    await expect(sheet.getByText(/iPhone/)).toHaveCount(0)
    // The first Escape closes the sheet, the second Settings.
    await page.keyboard.press('Escape')
    await expect(sheet).toBeHidden()
    await page.keyboard.press('Escape')
    await expect(page.getByRole('dialog')).toHaveCount(0)
  } finally {
    await browser.close()
  }
})
