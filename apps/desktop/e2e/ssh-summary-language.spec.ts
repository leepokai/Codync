import { test, expect } from '@playwright/test'
import type { SSHProfile } from '../src/shared/ssh.ts'
import { attach, setPref } from './sandbox.ts'

// 192.0.2.1 is TEST-NET-1: nothing answers there, so the connection stays pending.
const profile = (id: string): SSHProfile => ({ id, host: '192.0.2.1', name: id, port: 22, user: 'codync', identityFile: null, remotePort: 19222, computerId: null })

test('the roster header summarizes pending SSH computers in the app language (#517)', async () => {
  const { browser, page } = await attach()
  const id = 'e2e-ssh-517'
  try {
    await setPref(page, 'appLanguage', 'zh')
    expect(await page.evaluate((p) => window.codync.ssh.save(p), profile(id))).toBeNull()
    await page.evaluate((id) => window.codync.ssh.connect(id), id)
    // The header's title is its whole line, also when it shows only an icon.
    const header = page.locator('.computer-filter').first()
    await expect(header).toHaveAttribute('title', /1 台(正在连接…|需要处理)$/)
    await expect(header).not.toHaveAttribute('title', /connecting|attention|opening|not connected/)
  } finally {
    await page.evaluate((id) => { window.codync.ssh.disconnect(id); window.codync.ssh.remove(id) }, id)
    await setPref(page, 'appLanguage', 'en')
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await browser.close()
  }
})
