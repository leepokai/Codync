import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import { api, attach, sandbox } from './sandbox.ts'

type Usage = { services: { id: string }[] }

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

/** What Claude Code's status line hook posts for the login of `loginProfile` (the default login's with none): the
 *  session resets in `sessionMinutes`, the week in `weekHours` (a week less that many hours is where it stands). */
async function statusline(loginProfile: string | null, session: number, weekly: number, sessionMinutes: number, weekHours: number) {
  const { home, port } = sandbox()
  const now = Math.floor(Date.now() / 1000)
  const res = await fetch(`http://127.0.0.1:${port}/ingest/statusline${loginProfile ? `?loginProfile=${loginProfile}` : ''}`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${readFileSync(join(home, 'token'), 'utf8').trim()}` },
    body: JSON.stringify({
      rate_limits: {
        five_hour: { used_percentage: session, resets_at: now + sessionMinutes * 60 },
        seven_day: { used_percentage: weekly, resets_at: now + weekHours * 3600 },
      },
    }),
  })
  expect(res.status).toBe(204)
}

test("a bot's chat shows the plan limits of the login it runs on, with the week's day and pace", async () => {
  const { browser, page } = await attach()
  const run = Date.now().toString(36)
  const name = `Limits Bot ${run}`
  const service = ((await api<Usage>(page, 'usage', {})).services ?? []).find((s) => s.id === 'claude')
  test.skip(!service, 'Needs Claude Code installed on this computer.')
  if (!service) return
  let botId: string | undefined
  let profile: string | undefined
  const limits = page.locator('.plan-limits')
  try {
    await api(page, 'setUsageShown', { id: 'claude', shown: true })
    // Session: 2h55 left at 59%. Week: 6 days 1 hour left, so day 1 of 7, where a steady pace allows 14.3%.
    await statusline(null, 59, 26, 175, 6 * 24 + 1)
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', { name, backend: 'claude', connectors: [], skills: [], notify: false })
    botId = bot.id
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()

    // The labels follow the app's language: the readings, which don't, say which is which.
    const values = limits.locator('.plan-limit-value')
    // The host's own reading may add windows after them (a week for one model), never before.
    await expect(values.nth(0)).toHaveText('59%')
    await expect(values.nth(1)).toHaveText('26%')
    await expect(limits.locator('.plan-limit-reset').first()).toContainText('↻ 2h5')
    await expect(limits.locator('.plan-limit-day')).toHaveText('1/7')
    await expect(limits.locator('.plan-limit.over')).toHaveCount(1) // 26% on the first day is ahead of 14.3%
    await expect(limits.locator('.plan-limit-pace')).toHaveCount(1)

    // A bot on a login profile shows that login's readings, and leaves the default login's alone.
    const added = await api<{ loginProfile: { id: string } }>(page, 'addLoginProfile', { backend: 'claude', kind: 'account' })
    profile = added.loginProfile.id
    markSignedIn(profile)
    await api(page, 'renameLoginProfile', { backend: 'claude', loginProfile: profile, name: 'Work' }) // lists the login among the readings
    await statusline(profile, 91, 40, 45, 3 * 24)
    await api(page, 'updateBot', { id: botId, loginProfile: profile })
    await expect(values.nth(0)).toHaveText('91%')
    await expect(values.nth(1)).toHaveText('40%')
    await expect(limits.locator('.plan-limit-reset').first()).toHaveText(/↻ 4[45]m/) // 45 minutes, less the seconds since
    await expect(limits.locator('.plan-limit-day')).toHaveText('5/7') // 3 days left: day 5, where a steady pace allows 71.4%
    await expect(limits.locator('.plan-limit.over')).toHaveCount(1) // the session at 91%; 40% of the week is well within the pace
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    if (profile) await api(page, 'removeLoginProfile', { loginProfile: profile }).catch(() => {})
    await browser.close()
  }
})
