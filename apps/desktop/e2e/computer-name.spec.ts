import { test, expect } from '@playwright/test'
import { api, attach, openGeneralSettings } from './sandbox.ts'

type Hello = { name: string }

test('the user names this computer in Settings › Computers, and an empty name goes back to its hostname', async () => {
  const { browser, page } = await attach()
  const named = `Work Mac ${Date.now() % 100000}`
  let hostname: string | undefined
  try {
    // The name the host falls back to: clear any name an earlier run left.
    hostname = (await api<Hello>(page, 'setComputerName', { name: '' })).name
    await openGeneralSettings(page)
    const settings = page.getByRole('dialog')
    await settings.getByRole('button', { name: 'Computers', exact: true }).click()
    await expect(settings.getByText(hostname, { exact: true }).first()).toBeVisible()

    await settings.getByRole('button', { name: 'Computer name' }).first().click()
    const dialog = page.getByRole('alertdialog', { name: 'Computer name' })
    await expect(dialog.getByText('Leave it empty to use the hostname.')).toBeVisible()
    const field = dialog.getByRole('textbox', { name: 'Name' })
    await expect(field).toHaveValue(hostname)
    await field.fill(`  ${named}  `)
    await dialog.getByRole('button', { name: 'Save' }).click()
    await expect(dialog).toBeHidden()
    await expect(settings.getByText(named, { exact: true }).first()).toBeVisible()
    expect((await api<Hello>(page, 'hello', {})).name).toBe(named)

    // Emptied: Save still works and the card goes back to the hostname.
    await settings.getByRole('button', { name: 'Computer name' }).first().click()
    await field.fill('')
    await expect(dialog.getByRole('button', { name: 'Save' })).toBeEnabled()
    await dialog.getByRole('button', { name: 'Save' }).click()
    await expect(settings.getByText(hostname, { exact: true }).first()).toBeVisible()
    expect((await api<Hello>(page, 'hello', {})).name).toBe(hostname)

    // Named on another device (a phone, the terminal UI): the card follows the host's `name` event.
    await api(page, 'setComputerName', { name: named })
    await expect(settings.getByText(named, { exact: true }).first()).toBeVisible()
    await page.keyboard.press('Escape')
  } finally {
    await api(page, 'setComputerName', { name: '' }).catch(() => {})
    await browser.close()
  }
})
