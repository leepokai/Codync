import { test, expect, type Page } from '@playwright/test'
import { writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, sandbox } from './sandbox.ts'

type Bot = {
  id: string
  agentStep?: { agent: string; model: string | null } | null
  resting?: { reason: string }[]
}

/** A scripted agent that asks to sign in while `<name>.signedout` is in its folder. */
const relayAgent = resolve(__dirname, '../../../host/tests/fixtures/relay_agent.py')
const relay = (name: string) => `${fixtureCommand(relayAgent)} ${name}`

/** A computer in no account opens on the sign-in welcome; the roster is behind it. */
async function openRoster(page: Page) {
  const offline = page.getByRole('button', { name: 'Continue without an account', exact: false })
  await expect(page.locator('.nav-rail').or(offline).first()).toBeVisible()
  if (await offline.isVisible()) await offline.click()
  await expect(page.locator('.nav-rail')).toBeVisible()
}

/** A row by the exact bot name its own line shows. */
const row = (page: Page, name: string) =>
  page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })

/** The first line is the name alone, or with the computer's name when the roster spans computers. */
const heading = (name: string) => `${name}(?: · [^\\n]+)?`

test('the roster row tip names the bot, then the agent and model it runs', async () => {
  const { browser, page } = await attach()
  const run = Date.now()
  const codex = `Tip Codex ${run}`
  const bare = `Tip Bare ${run}`
  const made: string[] = []
  try {
    await openRoster(page)

    // One bot with a model, and one without: a bot with no model must keep a one-line tip.
    made.push((await api<{ bot: { id: string } }>(page, 'createBot', { name: codex, backend: 'codex', model: 'gpt-5.3-codex' })).bot.id)
    made.push((await api<{ bot: { id: string } }>(page, 'createBot', { name: bare, backend: 'codex' })).bot.id)

    // The tip is the app's own `title`: a line feed in it is a line break in the tooltip, so the
    // model lands on its own line under the name. Codex is the sandbox's agent name.
    // The heading may carry the computer's name when the roster spans several computers.
    await expect(row(page, codex)).toHaveAttribute('title', new RegExp(`^${heading(codex)}\\nCodex->gpt-5\\.3-codex$`))
    // And a bot with no model says nothing about one — no blank second line, no dangling agent.
    await expect(row(page, bare)).toHaveAttribute('title', new RegExp(`^${heading(bare)}$`))
  } finally {
    for (const id of made.reverse()) await api(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  }
})

test('a row on a named login profile names it between the agent and the model', async () => {
  const { browser, page } = await attach()
  const run = Date.now()
  const name = `Tip Login ${run}`
  let botId: string | undefined
  let profileId: string | undefined
  try {
    await openRoster(page)

    // The manager reads a login profile's name from this computer's hello; the roster must read the
    // same one. An API key profile needs no sign-in, so this names one without running an agent.
    const added = await api<{ loginProfile: { id: string } }>(page, 'addLoginProfile', { backend: 'codex', kind: 'apiKey' })
    profileId = added.loginProfile.id
    await api(page, 'renameLoginProfile', { backend: 'codex', loginProfile: profileId, name: 'Work' })
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'codex', model: 'gpt-5.3-codex', loginProfile: profileId,
    })
    botId = bot.id
    // The API calls don't touch the renderer's hello; a reload reads the new profile's name there.
    await page.reload()
    await openRoster(page)

    await expect(row(page, name)).toHaveAttribute('title', new RegExp(`^${heading(name)}\\nCodex->Work->gpt-5\\.3-codex$`))
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    if (profileId) await api(page, 'removeLoginProfile', { loginProfile: profileId }).catch(() => {})
    await browser.close()
  }
})

test('a fallback row tip names the agent and model answering now', async () => {
  const { browser, page } = await attach()
  const run = Date.now()
  const name = `Tip Fallback ${run}`
  let botId: string | undefined
  try {
    await openRoster(page)

    // Kimi Code is signed out and this computer doesn't run it; the scripted Codex stand-in answers,
    // on the fallback's own model. The row's logo and its tip must name the same agent.
    const { bot } = await api<{ bot: Bot }>(page, 'createBot', {
      name, backend: 'kimi', command: relay('primary'), connectors: [], skills: [], notify: false,
      fallbacks: [{ backend: 'codex', command: relay('standin'), model: 'e2e-model', modelOptions: {} }],
    })
    botId = bot.id
    writeFileSync(join(sandbox().home, 'bots', bot.id, 'workspace', 'primary.signedout'), '')
    await api(page, 'send', { botId: bot.id, text: 'Hello', clientNonce: `tip-${run}` })
    const current = async () => (await api<{ bot: Bot }>(page, 'updateBot', { id: bot.id })).bot
    await expect.poll(async () => {
      const now = await current()
      return [now.agentStep?.agent, now.agentStep?.model]
    }, { timeout: 30_000 }).toEqual(['codex', 'e2e-model'])

    await expect(row(page, name)).toHaveAttribute('title', new RegExp(`^${heading(name)}\\nCodex->e2e-model$`))
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
