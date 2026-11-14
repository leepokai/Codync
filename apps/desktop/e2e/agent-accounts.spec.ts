import { test, expect } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type Account = { id: string; name: string | null; signedIn: boolean }
type Hello = { backends: { id: string; name: string; installed?: boolean; accounts?: Account[] }[] }

test('an agent gets another account, a bot runs on it, and it goes once no bot does', async () => {
  const { browser, page } = await attach()
  const name = `Accounts e2e ${Date.now()}`
  let botId: string | undefined
  let accountId: string | undefined
  try {
    const { backends } = await api<Hello>(page, 'hello', {})
    const agent = backends.find((b) => b.accounts && b.installed)
    test.skip(!agent, 'Needs Claude Code or Codex installed on this computer.')
    if (!agent) return

    // Add account opens the new account's sign-in in a terminal on the computer; leaving it signs nothing in.
    const accountsOf = async () => (await api<Hello>(page, 'hello', {})).backends.find((b) => b.id === agent.id)?.accounts ?? []
    const before = new Set((await accountsOf()).map((a) => a.id))
    await page.getByRole('button', { name: /Extensions/ }).click()
    await page.locator('.agent-grid').getByText(agent.name, { exact: true }).click()
    await page.getByRole('button', { name: /Add account/ }).click()
    await expect(page.locator('.market-screen-title', { hasText: `Sign in to ${agent.name}` })).toBeVisible()
    await expect(page.locator('.setup-terminal-surface')).toBeVisible()
    await expect.poll(async () => (accountId = (await accountsOf()).find((a) => !before.has(a.id))?.id)).toBeTruthy()
    if (!accountId) return
    await page.locator('.market-screen-header').getByRole('button', { name: 'Back' }).click()
    await expect(page.locator('.agent-step-row', { hasText: 'New account' })).toContainText('Not signed in')
    // Its folder is its own login: signed out, whatever the computer's own is.
    expect((await api<{ signedIn: boolean }>(page, 'agentAuth', { backend: agent.id, account: accountId })).signedIn).toBe(false)

    const { bot } = await api<{ bot: { id: string; account?: string } }>(page, 'createBot', {
      name, backend: agent.id, account: accountId, connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    expect(bot.account).toBe(accountId)
    await expect(api(page, 'removeAgentAccount', { account: accountId })).rejects.toThrow() // a bot still runs on it

    // The bot's settings show the account, and move it back to the computer's own sign-in.
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: `${name}, view details` }).click()
    const picker = page.locator('.details-panel .agent-picker')
    await picker.locator('.agent-picker-summary').click()
    const account = picker.locator('.value-row', { hasText: 'Account' }).locator('.choice-picker')
    await expect(account).toHaveText('New account')
    await account.click()
    await page.getByRole('menuitemradio', { name: `This computer's sign-in` }).click()
    await expect(account).toHaveText(`This computer's sign-in`)

    // The agent's sheet lists the account and removes it, now that no bot runs on it.
    await page.getByRole('button', { name: /Extensions/ }).click()
    await page.locator('.agent-grid').getByText(agent.name, { exact: true }).click()
    const row = page.locator('.agent-step-row', { hasText: 'New account' })
    await expect(row).toContainText('Not signed in')
    await row.getByRole('button', { name: 'Remove account' }).click()
    await expect(row).toHaveCount(0)
    const after = await api<Hello>(page, 'hello', {})
    expect(after.backends.find((b) => b.id === agent.id)?.accounts?.some((a) => a.id === accountId)).toBe(false)
    accountId = undefined
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    if (accountId) await api(page, 'removeAgentAccount', { account: accountId }).catch(() => {})
    await page.reload() // Leaves Extensions, which would cover what the next spec clicks.
    await browser.close()
  }
})
