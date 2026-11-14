import { test, expect } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type Account = { id: string; name: string | null; signedIn: boolean }
type Hello = { backends: { id: string; name: string; installed?: boolean; accounts?: Account[]; accountSignIn?: string; ownAccount?: { label?: string | null } }[] }

const WORK = 'E2E work'
const OWN = 'E2E default'

test('an agent gets another account, a bot runs on it, and it goes once no bot does', async () => {
  const { browser, page } = await attach()
  const name = `Accounts e2e ${Date.now()}`
  let botId: string | undefined
  let accountId: string | undefined
  let agentId: string | undefined
  // The name the default account had before the test, put back after it.
  let ownLabel: string | null = null
  try {
    const { backends } = await api<Hello>(page, 'hello', {})
    // One whose accounts sign in in a terminal: a browser sign-in would open on this computer.
    const agent = backends.find((b) => b.accounts && b.installed && b.accountSignIn !== 'browser')
    test.skip(!agent, 'Needs Claude Code or Codex installed on this computer.')
    if (!agent) return
    agentId = agent.id
    ownLabel = agent.ownAccount?.label ?? null

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
    const added = page.locator('.agent-account', { hasText: 'New account' })
    await expect(added).toContainText('Not signed in')
    // The pencil names it; the default account, the login the computer already has, is named the same way.
    await added.getByRole('button', { name: 'Rename account' }).click()
    await page.getByRole('textbox', { name: 'Account name' }).fill(WORK)
    await page.getByRole('textbox', { name: 'Account name' }).press('Enter')
    await expect(page.locator('.agent-account', { hasText: WORK })).toContainText('Not signed in')
    const own = page.locator('.agent-account').first()
    await expect(own).toContainText('Default')
    await own.getByRole('button', { name: 'Rename account' }).click()
    await page.getByRole('textbox', { name: 'Account name' }).fill(OWN)
    await page.getByRole('textbox', { name: 'Account name' }).press('Enter')
    await expect(own).toContainText(OWN)
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
    await expect(account).toHaveText(WORK)

    // Add fallback offers the bot's own agent again, on a login the chain doesn't have: with the bot on the new
    // account, that is the default one.
    await page.locator('.details-panel').getByRole('button', { name: 'Add fallback' }).click()
    const adding = page.locator('.sheet-page', { hasText: 'Add fallback' })
    await adding.locator('.agent-picker-summary').click()
    await adding.locator('.agent-picker-choice').first().click()
    await adding.locator('.agent-picker-item').filter({ hasText: new RegExp(`^${agent.name}$`) }).click()
    await expect(adding.locator('.value-row', { hasText: 'Account' }).locator('.choice-picker')).toHaveText(OWN)
    await adding.getByRole('button', { name: 'Add', exact: true }).click()
    await expect(page.locator('.details-panel').getByText(`${agent.name} · Default`)).toBeVisible()
    const saved = async () => (await api<{ bot: { fallbacks: { backend: string; account?: string | null }[] } }>(page, 'updateBot', { id: botId })).bot.fallbacks
    await expect.poll(async () => (await saved()).map((f) => [f.backend, f.account ?? null])).toEqual([[agent.id, null]])

    // Back on the default account, that step is the bot's own login, so it goes.
    await account.click()
    await page.getByRole('menuitemradio', { name: OWN }).click()
    await expect(account).toHaveText(OWN)
    await expect.poll(saved).toHaveLength(0)

    // A fallback on the other account shows which one: the chain has each login once, not each agent.
    const stepped = await api<{ bot: { fallbacks: { backend: string; account?: string }[] } }>(page, 'updateBot', {
      id: botId, account: null, fallbacks: [{ backend: agent.id, account: accountId, model: null, modelOptions: {} }],
    })
    expect(stepped.bot.fallbacks).toMatchObject([{ backend: agent.id, account: accountId }])
    await expect(page.locator('.details-panel').getByText(`${agent.name} (${WORK}) · Default`)).toBeVisible()
    await expect(api(page, 'updateBot', {
      id: botId, fallbacks: [{ backend: agent.id, account: null, model: null, modelOptions: {} }], // without `account` the step keeps its saved one
    }).then((r) => (r as { bot: { fallbacks: unknown[] } }).bot.fallbacks)).resolves.toHaveLength(0) // the bot's own login again
    await api(page, 'updateBot', { id: botId, fallbacks: [] })

    // The agent's sheet lists the account and removes it, now that no bot runs on it.
    await page.getByRole('button', { name: /Extensions/ }).click()
    await page.locator('.agent-grid').getByText(agent.name, { exact: true }).click()
    const row = page.locator('.agent-account', { hasText: WORK })
    await expect(row).toContainText('Not signed in')
    await row.getByRole('button', { name: 'Remove account' }).click()
    await expect(row).toHaveCount(0)
    const after = await api<Hello>(page, 'hello', {})
    expect(after.backends.find((b) => b.id === agent.id)?.accounts?.some((a) => a.id === accountId)).toBe(false)
    accountId = undefined
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    if (accountId) await api(page, 'removeAgentAccount', { account: accountId }).catch(() => {})
    if (agentId) await api(page, 'renameAgentAccount', { backend: agentId, name: ownLabel ?? '' }).catch(() => {})
    await page.reload() // Leaves Extensions, which would cover what the next spec clicks.
    await browser.close()
  }
})
