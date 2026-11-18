import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { api, attach, openGeneralSettings, sandbox } from './sandbox.ts'

type Logins = { items: { id: string; site: string }[] }
const sites = async (page: Parameters<typeof api>[0]) => (await api<Logins>(page, 'credentialLogins', {})).items.map((l) => l.site)

// Settings › Computers › Saved credentials (docs/features/connector-credentials.md): Reset asks twice, then deletes
// every saved credential and the key they were sealed with; the next credential call makes a new key and works.
test('Saved credentials › Reset asks twice, deletes the saved credentials, and the next save works under a new key', async () => {
  const { browser, page } = await attach()
  const site = `e2e-reset-${Date.now()}.example.com`
  const key = join(sandbox().home, 'vault.key')
  try {
    await api(page, 'credentialSaveLogin', { site, username: 'e2e', password: 'e2e-not-a-secret' })
    expect(await sites(page)).toContain(site)
    const oldKey = readFileSync(key)

    await page.reload()
    await openGeneralSettings(page)
    const dialog = page.getByRole('dialog')
    await dialog.getByRole('button', { name: 'Computers', exact: true }).click()
    // This computer's resets sit together at the bottom. Reset everything asks twice too: cancel the second.
    const everything = dialog.locator('.settings-row', { hasText: 'All data' })
    await everything.getByRole('button', { name: 'Reset everything', exact: true }).click()
    await page.getByRole('alertdialog', { name: 'Reset all data?' }).getByRole('button', { name: 'Reset everything', exact: true }).click()
    const erase = page.getByRole('alertdialog', { name: "Delete all of Codync's data for good?" })
    await erase.getByRole('button', { name: 'Cancel', exact: true }).click()
    await expect(erase).toBeHidden()
    const row = dialog.locator('.settings-row', { hasText: 'Saved credentials' }).first()
    await expect(row.getByRole('button', { name: 'Reset', exact: true })).toHaveClass(/secondary/)
    await row.getByRole('button', { name: 'Reset', exact: true }).click()
    const ask = page.getByRole('alertdialog', { name: /^Reset saved credentials on / })
    await ask.getByRole('button', { name: 'Reset', exact: true }).click()
    // The second question: nothing is gone yet.
    const final = page.getByRole('alertdialog', { name: /for good\?$/ })
    await expect(final).toBeVisible()
    expect(await sites(page)).toContain(site)
    await final.getByRole('button', { name: 'Delete saved credentials', exact: true }).click()
    await expect(final).toBeHidden()

    // Gone, with its key; the call that read the list made a new key, and saving works again.
    await expect.poll(() => sites(page)).not.toContain(site)
    await api(page, 'credentialSaveLogin', { site, username: 'e2e', password: 'e2e-not-a-secret' })
    expect(await sites(page)).toContain(site)
    expect(readFileSync(key).equals(oldKey), 'a new key').toBe(false)
  } finally {
    const saved = (await api<Logins>(page, 'credentialLogins', {}).catch(() => ({ items: [] }))).items
    for (const l of saved.filter((l) => l.site === site)) await api(page, 'credentialRemoveLogin', { id: l.id }).catch(() => {})
    await page.reload() // Closes Settings, which would cover the next spec.
    await browser.close()
  }
})
