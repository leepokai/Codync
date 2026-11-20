import { test, expect } from '@playwright/test'
import type { SSHProfile } from '../src/shared/ssh.ts'
import { attach, setPref } from './sandbox.ts'

// 192.0.2.1 is TEST-NET-1: nothing answers there, so the connection stays pending.
const profile = (id: string): SSHProfile => ({ id, host: '192.0.2.1', name: id, port: 22, user: 'codync', identityFile: null, remotePort: 19222, computerId: null })

async function ensureWorkspace(page: import('@playwright/test').Page) {
  const offline = page.getByRole('button', { name: /Continue without an account|不登录继续/ })
  const header = page.locator('.computer-filter').first()
  await expect(header.or(offline).first()).toBeVisible()
  if (await offline.isVisible()) await offline.click()
  await expect(header).toBeVisible()
  return header
}

test('the roster header summarizes pending SSH computers in the app language (#517)', async () => {
  const { browser, page } = await attach()
  const id = 'e2e-ssh-517'
  try {
    await setPref(page, 'appLanguage', 'zh')
    const header = await ensureWorkspace(page)
    await page.evaluate(() => {
      window.codync.ssh.remove('e2e-ssh-517')
      window.codync.ssh.remove('e2e-ssh-521')
    })
    expect(await page.evaluate((p) => window.codync.ssh.save(p), profile(id))).toBeNull()
    await page.evaluate((id) => window.codync.ssh.connect(id), id)
    // The header's title is its whole line, also when it shows only an icon.
    await expect(header).toHaveAttribute('title', /1 台(正在连接…|需要处理)$/)
    await expect(header).not.toHaveAttribute('title', /connecting|attention|opening|not connected/)
  } finally {
    await page.evaluate((id) => { window.codync.ssh.disconnect(id); window.codync.ssh.remove(id) }, id)
    await setPref(page, 'appLanguage', 'en')
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await browser.close()
  }
})

test('each SSH profile row in the roster dropdown shows its status detail in the app language (#521)', async () => {
  const { browser, page } = await attach()
  const id = 'e2e-ssh-521'
  try {
    await setPref(page, 'appLanguage', 'zh')
    const header = await ensureWorkspace(page)
    await page.evaluate(() => {
      window.codync.ssh.remove('e2e-ssh-517')
      window.codync.ssh.remove('e2e-ssh-521')
    })
    expect(await page.evaluate((p) => window.codync.ssh.save(p), profile(id))).toBeNull()
    // save() starts connecting immediately; disconnect() returns the profile to idle.
    await page.evaluate((id) => window.codync.ssh.disconnect(id), id)
    await header.click()
    const row = page.locator('.menu-row', { hasText: id })
    await expect(row).toBeVisible()
    await expect(row.locator('.menu-detail')).toHaveText('未连接')
    await page.keyboard.press('Escape')
    await page.evaluate((id) => window.codync.ssh.connect(id), id)
    await expect(header).toHaveAttribute('title', /1 台(正在连接…|需要处理)$/)
    await header.click()
    await expect(row.locator('.menu-detail')).not.toContainText(/Not connected|Reading SSH settings|Reading the host key|Signing in|Opening the tunnel|Retrying…|Can't reach|couldn't/)
    await expect(row.locator('.menu-detail')).toContainText(/正在/)
    // Nothing answers at TEST-NET-1, so the host key scan gives up and the row retries with a translated reason.
    await expect(row.locator('.menu-detail')).toHaveText('无法连接到 192.0.2.1:22。 正在重试…', { timeout: 20_000 })
  } finally {
    await page.keyboard.press('Escape').catch(() => {})
    await page.evaluate((id) => { window.codync.ssh.disconnect(id); window.codync.ssh.remove(id) }, id)
    await setPref(page, 'appLanguage', 'en')
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await browser.close()
  }
})
