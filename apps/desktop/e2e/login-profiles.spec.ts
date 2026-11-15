import { test, expect } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type LoginProfile = { id: string; kind: 'account' | 'apiKey'; name: string | null; signedIn: boolean; vars: string[] }
type Hello = { backends: { id: string; name: string; installed?: boolean; loginProfiles?: LoginProfile[]; loginProfileSignIn?: string | null; defaultLoginProfile?: { label?: string | null } }[] }

const WORK = 'E2E work'
const OWN = 'E2E default'

test('an agent gets another login profile, a bot runs on it, and it goes once no bot does', async () => {
  const { browser, page } = await attach()
  const name = `Login profiles e2e ${Date.now()}`
  let botId: string | undefined
  let profileId: string | undefined
  let agentId: string | undefined
  // The name the default login profile had before the test, put back after it.
  let ownLabel: string | null = null
  try {
    const { backends } = await api<Hello>(page, 'hello', {})
    // One whose account profiles sign in in a terminal: a browser sign-in would open on this computer.
    const agent = backends.find((b) => b.loginProfiles && b.installed && b.loginProfileSignIn === 'terminal')
    test.skip(!agent, 'Needs Claude Code or Codex installed on this computer.')
    if (!agent) return
    agentId = agent.id
    ownLabel = agent.defaultLoginProfile?.label ?? null

    // Add account opens the new login profile's sign-in in a terminal on the computer; leaving it signs nothing in.
    const profilesOf = async () => (await api<Hello>(page, 'hello', {})).backends.find((b) => b.id === agent.id)?.loginProfiles ?? []
    const before = new Set((await profilesOf()).map((p) => p.id))
    await page.getByRole('button', { name: /Extensions/ }).click()
    await page.locator('.agent-grid').getByText(agent.name, { exact: true }).click()
    await page.getByRole('button', { name: /Add account/ }).click()
    await expect(page.locator('.market-screen-title', { hasText: `Sign in to ${agent.name}` })).toBeVisible()
    await expect(page.locator('.setup-terminal-surface')).toBeVisible()
    await expect.poll(async () => (profileId = (await profilesOf()).find((p) => !before.has(p.id))?.id)).toBeTruthy()
    if (!profileId) return
    await page.locator('.market-screen-header').getByRole('button', { name: 'Back' }).click()
    const added = page.locator('.login-profile', { hasText: 'New login profile' })
    await expect(added).toContainText('Not signed in')
    // The pencil names it; the default login profile, the login the computer already has, is named the same way.
    await added.getByRole('button', { name: 'Rename login profile' }).click()
    await page.getByRole('textbox', { name: 'Login profile name' }).fill(WORK)
    await page.getByRole('textbox', { name: 'Login profile name' }).press('Enter')
    await expect(page.locator('.login-profile', { hasText: WORK })).toContainText('Not signed in')
    const own = page.locator('.login-profile').first()
    await expect(own).toContainText('Default')
    await own.getByRole('button', { name: 'Rename login profile' }).click()
    await page.getByRole('textbox', { name: 'Login profile name' }).fill(OWN)
    await page.getByRole('textbox', { name: 'Login profile name' }).press('Enter')
    await expect(own).toContainText(OWN)
    // Its folder is its own login: signed out, whatever the computer's own is.
    expect((await api<{ signedIn: boolean }>(page, 'agentAuth', { backend: agent.id, loginProfile: profileId })).signedIn).toBe(false)

    const { bot } = await api<{ bot: { id: string; loginProfile?: string } }>(page, 'createBot', {
      name, backend: agent.id, loginProfile: profileId, connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    expect(bot.loginProfile).toBe(profileId)
    await expect(api(page, 'removeLoginProfile', { loginProfile: profileId })).rejects.toThrow() // a bot still runs on it

    // The bot's settings show the login profile, and move it back to the computer's own sign-in.
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: `${name}, view details` }).click()
    // Its own row under the picker, shown without opening it.
    const chosen = page.locator('.details-panel .value-row', { hasText: 'Login profile' }).locator('.choice-picker')
    await expect(chosen).toHaveText(WORK)

    // Add fallback offers the bot's own agent again, on a login the chain doesn't have: with the bot on the new
    // login profile, that is the default one.
    await page.locator('.details-panel').getByRole('button', { name: 'Add fallback' }).click()
    const adding = page.locator('.sheet-page', { hasText: 'Add fallback' })
    await expect(adding.locator('.agent-picker-summary')).toHaveCount(0) // the choices show at once
    await adding.locator('.agent-picker-choice').first().click()
    await adding.locator('.agent-picker-item').filter({ hasText: new RegExp(`^${agent.name}$`) }).click()
    await expect(adding.locator('.value-row', { hasText: 'Login profile' }).locator('.choice-picker')).toHaveText(OWN)
    await adding.getByRole('button', { name: 'Add', exact: true }).click()
    await expect(page.locator('.details-panel').getByText(`${agent.name} · Default`)).toBeVisible()
    const saved = async () => (await api<{ bot: { fallbacks: { backend: string; loginProfile?: string | null }[] } }>(page, 'updateBot', { id: botId })).bot.fallbacks
    await expect.poll(async () => (await saved()).map((f) => [f.backend, f.loginProfile ?? null])).toEqual([[agent.id, null]])

    // Back on the default login profile, that step is the bot's own login, so it goes.
    await chosen.click()
    await page.getByRole('menuitemradio', { name: OWN }).click()
    await expect(chosen).toHaveText(OWN)
    await expect.poll(saved).toHaveLength(0)

    // A fallback on the other login profile shows which one: the chain has each login once, not each agent.
    const stepped = await api<{ bot: { fallbacks: { backend: string; loginProfile?: string }[] } }>(page, 'updateBot', {
      id: botId, loginProfile: null, fallbacks: [{ backend: agent.id, loginProfile: profileId, model: null, modelOptions: {} }],
    })
    expect(stepped.bot.fallbacks).toMatchObject([{ backend: agent.id, loginProfile: profileId }])
    await expect(page.locator('.details-panel').getByText(`${agent.name} (${WORK}) · Default`)).toBeVisible()
    await expect(api(page, 'updateBot', {
      id: botId, fallbacks: [{ backend: agent.id, loginProfile: null, model: null, modelOptions: {} }], // without `loginProfile` the step keeps its saved one
    }).then((r) => (r as { bot: { fallbacks: unknown[] } }).bot.fallbacks)).resolves.toHaveLength(0) // the bot's own login again
    await api(page, 'updateBot', { id: botId, fallbacks: [] })

    // The agent's sheet lists the login profile and removes it, now that no bot runs on it.
    await page.getByRole('button', { name: /Extensions/ }).click()
    await page.locator('.agent-grid').getByText(agent.name, { exact: true }).click()
    const row = page.locator('.login-profile', { hasText: WORK })
    await expect(row).toContainText('Not signed in')
    await row.getByRole('button', { name: 'Remove login profile' }).click()
    await expect(row).toHaveCount(0)
    const after = await api<Hello>(page, 'hello', {})
    expect(after.backends.find((b) => b.id === agent.id)?.loginProfiles?.some((p) => p.id === profileId)).toBe(false)
    profileId = undefined
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    if (profileId) await api(page, 'removeLoginProfile', { loginProfile: profileId }).catch(() => {})
    if (agentId) await api(page, 'renameLoginProfile', { backend: agentId, name: ownLabel ?? '' }).catch(() => {})
    await page.reload() // Leaves Extensions, which would cover what the next spec clicks.
    await browser.close()
  }
})

// A made-up variable: nothing reads it, and its value is no real key.
const KEY_NAME = 'E2E_MADE_UP_KEY'

test('an API key login profile gets its keys from its sign-in card and its key form, and it goes', async () => {
  const { browser, page } = await attach()
  const name = `API key e2e ${Date.now()}`
  let botId: string | undefined
  let profileId: string | undefined
  try {
    const { backends } = await api<Hello>(page, 'hello', {})
    const agent = backends.find((b) => b.loginProfiles && b.installed)
    test.skip(!agent, 'Needs an agent installed on this computer.')
    if (!agent) return
    const profilesOf = async () => (await api<Hello>(page, 'hello', {})).backends.find((b) => b.id === agent.id)?.loginProfiles ?? []
    const before = new Set((await profilesOf()).map((p) => p.id))
    const header = page.locator('.market-screen-header')

    // Add API key makes the login profile and opens its key form.
    await page.getByRole('button', { name: /Extensions/ }).click()
    await page.locator('.agent-grid').getByText(agent.name, { exact: true }).click()
    await page.getByRole('button', { name: /Add API key/ }).click()
    await expect(page.locator('.market-screen-title', { hasText: 'API key' })).toBeVisible()
    await expect.poll(async () => (profileId = (await profilesOf()).find((p) => !before.has(p.id))?.id)).toBeTruthy()
    if (!profileId) return
    expect((await profilesOf()).find((p) => p.id === profileId)).toMatchObject({ kind: 'apiKey', vars: [] })
    await header.getByRole('button', { name: 'Back' }).click()

    // A bot on it is asked to sign in before its first message, since it has no key. The card opens the key form,
    // never a sign-in terminal.
    botId = (await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: agent.id, loginProfile: profileId, connectors: [], skills: [], notify: false,
    })).bot.id
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    const card = page.locator('.request-card')
    await card.getByRole('button', { name: 'Sign in' }).click()
    await expect(page.locator('.market-screen-title', { hasText: 'API key' })).toBeVisible()
    await expect(page.locator('.setup-terminal-surface')).toHaveCount(0)

    // A variable of the user's own: its name and value are typed, and Save goes back to the agent's sheet. A login
    // profile with a key counts as signed in, which completes the card.
    await page.getByRole('button', { name: 'Add variable' }).click()
    await page.getByRole('textbox', { name: 'Variable name' }).last().fill(KEY_NAME)
    await page.locator('.login-profile-key').last().getByLabel('Value').fill('made-up-e2e-value')
    await header.getByRole('button', { name: 'Save' }).click()
    await expect(page.locator('.login-profile', { hasText: KEY_NAME })).toContainText(`API key · ${KEY_NAME}`)
    expect((await profilesOf()).find((p) => p.id === profileId)?.vars).toEqual([KEY_NAME])
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(card).toContainText('Signed in')
    await api(page, 'deleteBot', { botId })
    botId = undefined

    // Edit keys opens the form again, on the saved variable (its value stays on the computer).
    await page.getByRole('button', { name: /Extensions/ }).click()
    await page.locator('.agent-grid').getByText(agent.name, { exact: true }).click()
    const row = page.locator('.login-profile', { hasText: KEY_NAME })
    await row.getByRole('button', { name: 'Edit keys' }).click()
    await expect(page.locator('.login-profile-key', { hasText: KEY_NAME }).getByLabel('Value')).toHaveAttribute('placeholder', 'Saved (type to replace)')

    // Import lists the login shell's variables by name, without the shell's own; leaving it imports nothing.
    await page.getByRole('button', { name: 'Import from login shell' }).click()
    await expect(page.locator('.market-screen-title', { hasText: 'Import from login shell' })).toBeVisible()
    await expect(page.getByText('Pick the variables this login profile should use.', { exact: false })).toBeVisible()
    const { names } = await api<{ names: string[] }>(page, 'loginShellVariables', {})
    expect(names).not.toContain('PATH')
    for (const shown of names.slice(0, 3)) await expect(page.getByText(shown, { exact: true }).first()).toBeVisible()
    await header.getByRole('button', { name: 'Back' }).click()
    await header.getByRole('button', { name: 'Back' }).click()
    expect((await profilesOf()).find((p) => p.id === profileId)?.vars).toEqual([KEY_NAME])

    await row.getByRole('button', { name: 'Remove login profile' }).click()
    await expect(row).toHaveCount(0)
    expect((await profilesOf()).some((p) => p.id === profileId)).toBe(false)
    profileId = undefined
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    if (profileId) await api(page, 'removeLoginProfile', { loginProfile: profileId }).catch(() => {})
    await page.reload() // Leaves Extensions, which would cover what the next spec clicks.
    await browser.close()
  }
})
