import { test, expect } from '@playwright/test'
import { api, attach } from './sandbox.ts'

test('the roster row tip names the bot, then the agent and model it runs', async () => {
  const { browser, page } = await attach()
  const run = Date.now()
  const codex = `Tip Codex ${run}`
  const bare = `Tip Bare ${run}`
  const made: string[] = []
  const row = (name: string) => page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })
  try {
    // A computer in no account opens on the sign-in welcome; the roster is behind it.
    const offline = page.getByRole('button', { name: 'Continue without an account', exact: false })
    await expect(page.locator('.nav-rail').or(offline).first()).toBeVisible()
    if (await offline.isVisible()) await offline.click()
    await expect(page.locator('.nav-rail')).toBeVisible()

    // One bot with a model, and one without: a bot with no model must keep a one-line tip.
    made.push((await api<{ bot: { id: string } }>(page, 'createBot', { name: codex, backend: 'codex', model: 'gpt-5.3-codex' })).bot.id)
    made.push((await api<{ bot: { id: string } }>(page, 'createBot', { name: bare, backend: 'codex' })).bot.id)

    // The tip is the app's own `title`: a line feed in it is a line break in the tooltip, so the
    // model lands on its own line under the name. Codex is the sandbox's agent name.
    await expect(row(codex)).toHaveAttribute('title', new RegExp(`^${codex}\nCodex->gpt-5.3-codex$`))
    // And a bot with no model says nothing about one — no blank second line, no dangling agent.
    await expect(row(bare)).toHaveAttribute('title', new RegExp(`^${bare}$`))
  } finally {
    for (const id of made.reverse()) await api(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  }
})
