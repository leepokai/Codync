import { test, expect } from '@playwright/test'
import { spawnSync } from 'node:child_process'
import { existsSync, statSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { SANDBOX_PREFIX } from '../src/shared/dev-sandbox.ts'
import { attach, sandbox } from './sandbox.ts'

test('a sandbox keeps a saved sign-in under a key in its own folder and leaves Keychain alone', async () => {
  const { browser, page } = await attach()
  const site = `e2e-${Date.now()}.example.com`
  const { name } = sandbox()
  const row = page.getByText(site, { exact: true })
  try {
    await page.reload()
    await page.getByRole('button', { name: /Extensions/ }).click()
    await page.getByRole('button', { name: 'Credentials' }).click()
    await page.getByPlaceholder('github.com').fill(site)
    await page.getByPlaceholder('name@example.com').fill('e2e')
    await page.getByPlaceholder('Password or op:// reference').fill('e2e-not-a-secret')
    await page.getByRole('button', { name: 'Save sign-in' }).click()
    await expect(row).toBeVisible()

    const key = join(homedir(), SANDBOX_PREFIX + name, 'vault.key')
    expect(existsSync(key)).toBe(true)
    if (process.platform !== 'win32') expect(statSync(key).mode & 0o777).toBe(0o600)
    if (process.platform === 'darwin') {
      // Without -w, so this reads whether the item exists and never its secret.
      const item = spawnSync('security', ['find-generic-password', '-s', `Codync Dev (${name}) Safe Storage`])
      expect(item.status, 'the sandbox app left a Safe Storage item').not.toBe(0)
    }
  } finally {
    // Removes the sign-in again, so a sandbox's list doesn't grow with each run.
    await row.locator('xpath=ancestor::*[.//button[@title="Remove"]][1]').getByTitle('Remove').click().catch(() => {})
    await page.reload()
    await browser.close()
  }
})
