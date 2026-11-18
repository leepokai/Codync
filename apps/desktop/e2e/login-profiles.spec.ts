import { test, expect, type Page } from '@playwright/test'
import { addAccount, api, attach, loginProfilesOf, openAgentSheet, standInAgent, typeMessage } from './sandbox.ts'

// Both tests run on the stand-in Devin CLI of an e2e sandbox (`npm run dev -- --cdp`,
// docs/guides/test-stack.md#stand-in-agents): its sign-in and its agent are that script, so nothing here starts a
// real agent or touches a real account (login-profiles-real.spec.ts opens a real agent's sign-in, only when asked).
test.skip(process.platform === 'win32', 'The stand-in agent is a script: not on Windows.')

const WORK = 'E2E work'
const OWN = 'E2E default'

/**
 * The open bot's only fallback, opened in place (#377): it shows the step on `loginProfile`, the agent's default model
 * is picked and saved. Returns the chain the host has then, as [agent, login profile, model].
 */
async function editFallbackModel(page: Page, botId: string, loginProfile: string) {
  const step = page.locator('.details-panel .fallback-number').locator('..')
  // The row stays open by its place (a step + just added still is), so it is opened only when it's closed.
  const summary = step.locator('.agent-picker-summary')
  if ((await summary.getAttribute('aria-expanded')) === 'false') await summary.click()
  await expect(step.locator('.value-row', { hasText: 'Login profile' }).locator('.choice-picker')).toHaveText(loginProfile)
  const model = step.locator('.agent-picker-choice').nth(1)
  if ((await model.getAttribute('aria-expanded')) === 'false') await model.click()
  await step.locator('.agent-picker-item').first().click()
  let chain: (string | null)[][] = []
  await expect.poll(async () => {
    const { bot } = await api<{ bot: { fallbacks: { backend: string; loginProfile?: string | null; model?: string | null }[] } }>(page, 'updateBot', { id: botId })
    chain = bot.fallbacks.map((f) => [f.backend, f.loginProfile ?? null, f.model ?? null])
    return chain.some((step) => step[2] === 'e2e-model')
  }).toBe(false)
  return chain
}

test('an agent gets another login profile, it signs in, a bot runs on it, and it goes once no bot does', async () => {
  const { browser, page } = await attach()
  const name = `Login profiles e2e ${Date.now()}`
  let botId: string | undefined
  let profileId: string | undefined
  let agentId: string | undefined
  // The name the default login profile had before the test, put back after it.
  let ownLabel: string | null = null
  try {
    const agent = await standInAgent(page, 'devin')
    agentId = agent.id
    ownLabel = agent.defaultLoginProfile?.label ?? null

    // Add account opens the new login profile's sign-in in a terminal on the computer; leaving it signs nothing in.
    await addAccount(page, agent, (id) => (profileId = id))
    // The pencil names it; the default login profile, the login the computer already has, is named the same way.
    await page.locator('.login-profile', { hasText: 'New login profile' }).getByRole('button', { name: 'Rename login profile' }).click()
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

    // Its row opens the sign-in again. Finished in the terminal, the login profile is signed in.
    const work = page.locator('.login-profile', { hasText: WORK })
    await work.getByRole('button', { name: 'Sign in' }).click()
    await expect(page.locator('.setup-terminal-surface')).toContainText('press Enter to sign in')
    await page.keyboard.press('Enter')
    await page.getByRole('button', { name: 'Done' }).click()
    await expect(work).toContainText('Signed in')
    await expect(work.getByRole('button', { name: 'Sign in' })).toHaveCount(0)

    const { bot } = await api<{ bot: { id: string; loginProfile?: string } }>(page, 'createBot', {
      name, backend: agent.id, loginProfile: profileId, connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    expect(bot.loginProfile).toBe(profileId)
    await expect(api(page, 'removeLoginProfile', { loginProfile: profileId })).rejects.toThrow() // a bot still runs on it

    // The bot's settings show the login profile, and move it back to the computer's own sign-in.
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    // Its agent runs on the login profile's own sign-in.
    await typeMessage(page, 'hi')
    await expect(page.getByRole('log', { name: 'Messages' }).getByText(`Devin stand-in on ${profileId}, signed in`)).toBeVisible()
    await page.getByRole('button', { name: `${name}, view details` }).click()
    await page.locator('.details-panel').getByRole('tab', { name: 'Settings' }).click()
    // Its own row under the picker, shown without opening it.
    // The bot's own comes before any fallback's.
    const chosen = page.locator('.details-panel .value-row', { hasText: 'Login profile' }).first().locator('.choice-picker')
    await expect(chosen).toHaveText(WORK)

    // + adds a step, open in place, that can be the bot's own agent again, on a login the chain doesn't have: with the
    // bot on the new login profile, that is the default one.
    await page.locator('.details-panel').getByRole('button', { name: 'Add fallback' }).click()
    const adding = page.locator('.details-panel .fallback-number').locator('..')
    await adding.locator('.agent-picker-choice').first().click()
    await adding.locator('.agent-picker-item').filter({ hasText: new RegExp(`^${agent.name}$`) }).click()
    await expect(adding.locator('.value-row', { hasText: 'Login profile' }).locator('.choice-picker')).toHaveText(OWN)
    const saved = async () => (await api<{ bot: { fallbacks: { backend: string; loginProfile?: string | null }[] } }>(page, 'updateBot', { id: botId })).bot.fallbacks
    await expect.poll(async () => (await saved()).map((f) => [f.backend, f.loginProfile ?? null])).toEqual([[agent.id, null]])

    // Back on the default login profile, that step is the bot's own login, so it goes.
    await chosen.click()
    await page.getByRole('menuitemradio', { name: OWN }).click()
    await expect(chosen).toHaveText(OWN)
    await expect.poll(saved).toHaveLength(0)

    // A fallback on the other login profile shows which one: the chain has each login once, not each agent.
    const stepped = await api<{ bot: { fallbacks: { backend: string; loginProfile?: string }[] } }>(page, 'updateBot', {
      id: botId, loginProfile: null, fallbacks: [{ backend: agent.id, loginProfile: profileId, model: 'e2e-model', modelOptions: {} }],
    })
    expect(stepped.bot.fallbacks).toMatchObject([{ backend: agent.id, loginProfile: profileId }])
    const summary = page.locator('.details-panel .fallback-number').locator('..').locator('.agent-picker-summary')
    await expect(summary).toContainText(`${agent.name} (${WORK})`)
    await expect(summary).toContainText('e2e-model')
    // Opened, the step keeps its own login: another model, and it stays on its login profile.
    expect(await editFallbackModel(page, botId, WORK)).toEqual([[agent.id, profileId, null]])
    await expect(summary).toContainText(`${agent.name} (${WORK})`)
    await expect(api(page, 'updateBot', {
      id: botId, fallbacks: [{ backend: agent.id, loginProfile: null, model: null, modelOptions: {} }], // without `loginProfile` the step keeps its saved one
    }).then((r) => (r as { bot: { fallbacks: unknown[] } }).bot.fallbacks)).resolves.toHaveLength(0) // the bot's own login again
    // Back on the default login, the bot's agent runs on that one.
    await api(page, 'updateBot', { id: botId, fallbacks: [] })
    await typeMessage(page, 'hi again')
    await expect(page.getByRole('log', { name: 'Messages' }).getByText('Devin stand-in on the default login, signed out')).toBeVisible()

    // The agent's sheet lists the login profile and removes it, now that no bot runs on it.
    await openAgentSheet(page, agent.name)
    const row = page.locator('.login-profile', { hasText: WORK })
    await expect(row).toContainText('Signed in')
    await row.getByRole('button', { name: 'Remove login profile' }).click()
    await expect(row).toHaveCount(0)
    expect((await loginProfilesOf(page, agent.id)).some((p) => p.id === profileId)).toBe(false)
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
    const agent = await standInAgent(page, 'devin')
    const profilesOf = () => loginProfilesOf(page, agent.id)
    const before = new Set((await profilesOf()).map((p) => p.id))
    const header = page.locator('.market-screen-header')

    // Add API key makes the login profile and opens its key form.
    await openAgentSheet(page, agent.name)
    await page.getByRole('button', { name: /Add API key/ }).click()
    await expect(page.locator('.market-screen-title', { hasText: 'API key' })).toBeVisible()
    await expect.poll(async () => (profileId = (await profilesOf()).find((p) => !before.has(p.id))?.id)).toBeTruthy()
    if (!profileId) return
    expect((await profilesOf()).find((p) => p.id === profileId)).toMatchObject({ kind: 'apiKey', vars: [] })
    await header.getByRole('button', { name: 'Back' }).click()

    // A fallback on it keeps it through Edit, although a login profile that isn't signed in is no choice for a new step.
    botId = (await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'kimi', connectors: [], skills: [], notify: false,
      fallbacks: [{ backend: agent.id, loginProfile: profileId, model: 'e2e-model', modelOptions: {} }],
    })).bot.id
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: `${name}, view details` }).click()
    await page.locator('.details-panel').getByRole('tab', { name: 'Settings' }).click()
    expect(await editFallbackModel(page, botId, 'New login profile')).toEqual([[agent.id, profileId, null]])
    await api(page, 'deleteBot', { botId })
    botId = undefined

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
    await openAgentSheet(page, agent.name)
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
