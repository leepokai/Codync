import { test, expect, type Page } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import { api, attach, sandbox, setPref } from './sandbox.ts'

type Window = { id: string; percent: number }
type LoginProfile = { id: string; name: string; windows: Window[]; updatedAt: number; failedAt?: number | null }
type Provider = { id: string; windows: Window[]; updatedAt: number; failedAt?: number | null; loginProfiles?: LoginProfile[] }
type Usage = { providers: Provider[]; services: { id: string; shown: boolean }[] }

function markSignedIn(id: string) {
  const db = new DatabaseSync(join(sandbox().home, 'codync.db'))
  try {
    const row = db.prepare("SELECT v FROM kv WHERE k = 'login-profiles'").get() as { v: string }
    const profiles = (JSON.parse(row.v) as { id: string; signedIn: boolean }[]).map((p) => (p.id === id ? { ...p, signedIn: true } : p))
    db.prepare("UPDATE kv SET v = ? WHERE k = 'login-profiles'").run(JSON.stringify(profiles))
  } finally {
    db.close()
  }
}

async function statusline(loginProfile: string | null, session: number, weekly: number) {
  const { home, port } = sandbox()
  const resets_at = Math.floor(Date.now() / 1000) + 3600
  const res = await fetch(`http://127.0.0.1:${port}/ingest/statusline${loginProfile ? `?loginProfile=${loginProfile}` : ''}`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${readFileSync(join(home, 'token'), 'utf8').trim()}` },
    body: JSON.stringify({ rate_limits: { five_hour: { used_percentage: session, resets_at }, seven_day: { used_percentage: weekly, resets_at } } }),
  })
  expect(res.status).toBe(204)
}

async function openUsage(page: Page) {
  await page.reload()
  const skip = page.getByRole('button', { name: /Continue without|不登录继续/ })
  if (await skip.isVisible().catch(() => false)) await skip.click()
  await page.locator('.sidebar-account').click()
  await page.locator('.account-panel').getByText(/Usage|用量/).click()
}

test('a failed usage check keeps the previous windows marked out of date until a reading lands (#516)', async () => {
  test.setTimeout(90_000)
  const { browser, page } = await attach()
  const usage = () => api<Usage>(page, 'usage', {})
  const claude = async () => (await usage()).providers.find((p) => p.id === 'claude')
  const service = (await usage()).services.find((s) => s.id === 'claude')
  test.skip(!service, 'Needs Claude Code installed on this computer.')
  if (!service) return
  let profile: string | undefined
  const wasIsland = await page.evaluate(() => window.codync.island.enabled())
  try {
    await setPref(page, 'appLanguage', 'en')
    const skip = page.getByRole('button', { name: /Continue without|不登录继续/ })
    if (await skip.isVisible().catch(() => false)) await skip.click()
    await api(page, 'setUsageShown', { id: 'claude', shown: true })
    await statusline(null, 12, 34)

    const added = await api<{ loginProfile: { id: string } }>(page, 'addLoginProfile', { backend: 'claude', kind: 'account' })
    profile = added.loginProfile.id
    markSignedIn(profile)
    await api(page, 'renameLoginProfile', { backend: 'claude', loginProfile: profile, name: 'Stale-516' })

    // Wait for the initial unread check on the empty Stale-516 profile to finish before seeding its statusline.
    await expect
      .poll(async () => (await claude())?.loginProfiles?.find((p) => p.id === profile)?.updatedAt ?? 0, { timeout: 45_000 })
      .toBeGreaterThan(0)

    await statusline(profile, 91, 40)
    // Refresh triggers a probe on the empty Stale-516 profile, which fails while keeping the 91%/40% windows.
    const refreshed = await api<Usage>(page, 'usage', { refresh: true })
    const workRow = refreshed.providers.find((p) => p.id === 'claude')?.loginProfiles?.find((p) => p.id === profile)
    expect(workRow?.failedAt).toBeTruthy()
    expect(workRow?.windows.map((w) => w.percent)).toEqual([91, 40])

    await openUsage(page)
    const agent = page.locator('.settings-usage-provider', { hasText: 'Claude' })
    const workGroup = agent.locator('.settings-usage-login', { hasText: 'Stale-516' })
    await expect(workGroup.locator('.settings-usage-stale')).toContainText(/^Out of date · check failed /)
    await expect(workGroup.locator('.settings-usage-stale-windows')).toHaveCount(2)
    await expect(workGroup).toContainText('91%')
    await expect(workGroup).toContainText('40%')

    if (process.env.USAGE_STALE_SHOT_EN) {
      await page.screenshot({ path: process.env.USAGE_STALE_SHOT_EN })
    }

    // Island shows the tightest login (Work at 91%) marked Out of date.
    await page.evaluate(() => window.codync.island.setEnabled(true))
    let islandPage: Page | undefined
    await expect
      .poll(() => {
        islandPage = browser
          .contexts()
          .flatMap((c) => c.pages())
          .find((p) => p !== page && !p.url().startsWith('devtools://'))
        return islandPage?.url() ?? ''
      }, { timeout: 10_000 })
      .not.toBe('')
    if (islandPage) {
      const closedIsland = islandPage.locator('.island[role="button"]')
      if ((await closedIsland.count()) > 0) {
        await closedIsland.dispatchEvent('click')
      }
      await expect(islandPage.locator('.island-meter-stale')).toBeVisible({ timeout: 10_000 })
      await islandPage.locator('.island-meter-stale').click()
      await expect(islandPage.locator('.island-usage-stale')).toHaveText('Out of date')
      await expect(islandPage.locator('.island-usage-row.stale').first()).toContainText('91%')
      if (process.env.USAGE_STALE_SHOT_ISLAND) {
        await islandPage.screenshot({ path: process.env.USAGE_STALE_SHOT_ISLAND })
      }
    }

    // Simplified Chinese translation check.
    await setPref(page, 'appLanguage', 'zh')
    await expect(workGroup.locator('.settings-usage-stale')).toContainText(/^已过期 · .*检查失败/)
    if (process.env.USAGE_STALE_SHOT_ZH) {
      await page.screenshot({ path: process.env.USAGE_STALE_SHOT_ZH })
    }
    if (islandPage) {
      await expect(islandPage.locator('.island-usage-stale')).toHaveText('已过期')
    }

    // A fresh reading clears failedAt and removes the stale markings.
    await statusline(profile, 92, 41)
    await expect(workGroup.locator('.settings-usage-stale')).toHaveCount(0)
    await expect(workGroup.locator('.settings-usage-stale-windows')).toHaveCount(0)
    await expect(workGroup).toContainText('92%')
    await expect(workGroup).toContainText('41%')
    expect((await claude())?.loginProfiles?.find((p) => p.id === profile)?.failedAt).toBeUndefined()
  } finally {
    await page.evaluate((on) => window.codync.island.setEnabled(on), wasIsland).catch(() => {})
    if (profile) {
      await expect(() => api(page, 'removeLoginProfile', { loginProfile: profile })).toPass({ timeout: 20_000 }).catch(() => {})
    }
    await api(page, 'setUsageShown', { id: 'claude', shown: service.shown }).catch(() => {})
    await setPref(page, 'appLanguage', 'en').catch(() => {})
    await page.evaluate(() => localStorage.removeItem('appLanguage')).catch(() => {})
    await page.reload().catch(() => {})
    await browser.close()
  }
})
