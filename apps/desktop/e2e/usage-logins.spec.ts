import { test, expect, type Page } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import { api, attach, sandbox } from './sandbox.ts'

type Window = { id: string; percent: number }
type Provider = { id: string; windows: Window[]; loginProfiles?: { id: string; name: string; windows: Window[] }[] }
type Usage = { providers: Provider[]; services: { id: string; shown: boolean }[] }

/** Marks a login profile signed in, in the sandbox host's own store: no agent is signed in for the test. */
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

/** What Claude Code's status line hook posts, for the session of `loginProfile` or of the default login. */
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
  await page.reload() // A settings window left open would cover the account button.
  await page.locator('.sidebar-account').click()
  await page.locator('.account-panel').getByText('Usage', { exact: true }).click()
}

test('Settings > Usage lists each login of an agent with login profiles, with its own limits', async () => {
  const { browser, page } = await attach()
  const usage = () => api<Usage>(page, 'usage', {})
  const claude = async () => (await usage()).providers.find((p) => p.id === 'claude')
  const service = (await usage()).services.find((s) => s.id === 'claude')
  test.skip(!service, 'Needs Claude Code installed on this computer.')
  if (!service) return
  let profile: string | undefined
  try {
    await api(page, 'setUsageShown', { id: 'claude', shown: true })
    await statusline(null, 12, 34)
    await openUsage(page)
    const agent = page.locator('.settings-usage-provider', { hasText: 'Claude' })
    // On one login the agent has no groups.
    await expect(agent).toContainText('12%')
    await expect(page.locator('.settings-usage-login')).toHaveCount(0)

    const added = await api<{ loginProfile: { id: string } }>(page, 'addLoginProfile', { backend: 'claude', kind: 'account' })
    profile = added.loginProfile.id
    markSignedIn(profile)
    await api(page, 'renameLoginProfile', { backend: 'claude', loginProfile: profile, name: 'Work' })
    expect((await claude())?.loginProfiles).toMatchObject([{ id: profile, name: 'Work', windows: [] }])
    const groups = agent.locator('.settings-usage-login')
    await expect(groups).toHaveCount(2)
    await expect(groups.nth(0)).toContainText('12%')
    await expect(groups.nth(1)).toContainText('Work')
    await expect(groups.nth(1)).toContainText('No reading yet')

    // A reading lands on the login its session runs on, and leaves the default login's alone.
    await statusline(profile, 91, 40)
    await expect(groups.nth(1)).toContainText('91%')
    await expect(groups.nth(1)).toContainText('40%')
    await expect(groups.nth(1)).not.toContainText('No reading yet')
    await expect(groups.nth(0)).toContainText('12%')
    await expect(groups.nth(0)).toContainText('34%')
    await statusline(null, 13, 34)
    await expect(groups.nth(0)).toContainText('13%')
    await expect(groups.nth(1)).toContainText('91%')

    await api(page, 'renameLoginProfile', { backend: 'claude', loginProfile: profile, name: 'Side' })
    await expect(groups.nth(1)).toContainText('Side')
    await expect(groups.nth(1)).not.toContainText('Work')

    // Refused while the new row's first usage check still runs the agent's CLI on it.
    await expect(() => api(page, 'removeLoginProfile', { loginProfile: profile })).toPass({ timeout: 20_000 })
    profile = undefined
    await expect(groups).toHaveCount(0)
    await expect(agent).toContainText('13%')
    expect((await claude())?.loginProfiles).toBeUndefined()
  } finally {
    if (profile) await api(page, 'removeLoginProfile', { loginProfile: profile }).catch(() => {})
    await api(page, 'setUsageShown', { id: 'claude', shown: service.shown }).catch(() => {})
    await page.reload().catch(() => {}) // Closes Settings, which would cover the next spec's window.
    await browser.close()
  }
})
