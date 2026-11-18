import { chromium, expect, type Browser, type Page } from '@playwright/test'
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { SANDBOX_PREFIX, sandboxName } from '../src/shared/dev-sandbox.ts'
import type { HostSnapshot } from '../src/shared/ipc.ts'

// Playwright loads specs as CommonJS here (the package has no "type": "module").
const root = resolve(__dirname, '../../..')
// A scripted agent whose turn waits until a steered message joins it (host/tests/fixtures/steer_agent.py).
const steerAgent = join(root, 'host/tests/fixtures/steer_agent.py')
// A scripted agent that answers each message with its text (host/tests/fixtures/mode_agent.py).
const echoAgent = join(root, 'host/tests/fixtures/mode_agent.py')
// A scripted agent that puts items in its box and is the task that works them (host/tests/fixtures/task_agent.py).
export const taskAgent = join(root, 'host/tests/fixtures/task_agent.py')
// The stand-in agent CLIs a `--cdp` sandbox's host finds first on its PATH (host/tests/fixtures/bin/README.md).
const standIns = join(root, 'host/tests/fixtures/bin')

/** This worktree's sandbox: its name, data folder, host port, DevTools port (`npm run dev -- --cdp` opens port + 10000) and whether it runs headless. */
export function sandbox() {
  const name = sandboxName(basename(root))
  const home = join(homedir(), SANDBOX_PREFIX + name)
  const record = join(home, 'sandbox.json')
  let port: number
  let headless: boolean
  try {
    ;({ port, headless = false } = JSON.parse(readFileSync(record, 'utf8')) as { port: number; headless?: boolean })
  } catch {
    throw new Error(`No sandbox for ${root}: run \`npm run dev -- --cdp\` in apps/desktop first.`)
  }
  return { name, home, port, cdp: port + 10000, headless }
}

/** Freezes the sandbox host, still connected and answering nothing, until the function it returns. Not on Windows. */
export function freezeHost() {
  const pid = Number(execFileSync('lsof', ['-t', '-sTCP:LISTEN', `-iTCP:${sandbox().port}`], { encoding: 'utf8' }).trim())
  process.kill(pid, 'SIGSTOP')
  return () => process.kill(pid, 'SIGCONT')
}

/** The process ID of the app `browser` is attached to (its main process). */
export async function appPid(browser: Browser) {
  const session = await browser.newBrowserCDPSession()
  const { processInfo } = await session.send('SystemInfo.getProcessInfo')
  await session.detach()
  const main = processInfo.find((p) => p.type === 'browser')
  if (!main) throw new Error('The attached app reported no main process.')
  return main.id
}

/** Attaches to the sandbox app's main window over the DevTools protocol. */
export async function attach(): Promise<{ browser: Browser; page: Page }> {
  const { cdp } = sandbox()
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${cdp}`).catch(() => {
    throw new Error(`The sandbox app isn't listening on ${cdp}: start it with \`npm run dev -- --cdp\`.`)
  })
  const page = browser
    .contexts()
    .flatMap((context) => context.pages())
    .find((p) => !p.url().startsWith('devtools://'))
  if (!page) throw new Error('The sandbox app has no window open.')
  await setPref(page, 'githubStarAsk', { kind: 'done' }) // Its sheet would cover what a test clicks.
  await setPref(page, 'kofiAsk', { asked: 3, day: 0 }) // So would the Ko-fi invitation's, on a sandbox used for days.
  // So would the ask to add a computer in no account, on a signed-in `--cloud` sandbox, and a Ko-fi invitation already up.
  for (const sheet of [page.getByRole('alertdialog'), page.getByRole('dialog', { name: 'Support Codync' })]) {
    const notNow = sheet.getByRole('button', { name: 'Not now' })
    if (await notNow.isVisible()) await notNow.click()
  }
  return { browser, page }
}

/** Sets a `pref` in the running window, which hears it as if another window had written it. */
export async function setPref(page: Page, key: string, value: unknown) {
  await page.evaluate(([key, value]) => {
    localStorage.setItem(key, value)
    window.dispatchEvent(new StorageEvent('storage', { key, newValue: value }))
  }, [key, JSON.stringify(value)] as const)
}

/** A bot on the scripted agent, which waits up to `seconds` for a steered message. */
export async function steerBot(page: Page, name: string, seconds: number) {
  const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
    name, backend: 'custom', command: `${fixtureCommand(steerAgent)} steer ${seconds}`, connectors: [], skills: [], notify: false,
  })
  return bot
}

/** A bot on the scripted agent that answers each message with `mode read-only · <the message>`, in `cwd` when given (the agent logs its calls to calls.jsonl there). */
export async function echoBot(page: Page, name: string, cwd?: string) {
  const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
    name, backend: 'custom', command: fixtureCommand(echoAgent), cwd, connectors: [], skills: [], notify: false,
  })
  return bot
}

/** A bot on the scripted agent that adds an item to its box for `ADD <title>|<brief>`; it asks before it acts. As
 * `backend` (an agent the host knows by name gets that agent's sign-in card when it needs one). */
export async function taskBot(page: Page, name: string, backend = 'custom') {
  const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
    name, backend, command: fixtureCommand(taskAgent), connectors: [], skills: [], notify: false,
  })
  await api(page, 'updateBot', { id: bot.id, permission: 'ask' })
  return bot
}

type LoginProfile = { id: string; kind: 'account' | 'apiKey'; name: string | null; signedIn: boolean; vars: string[] }
export type Agent = { id: string; name: string; path?: string | null; installed?: boolean; loginProfiles?: LoginProfile[]; defaultLoginProfile?: { label?: string | null } }

/** The agents the sandbox host knows, as its `hello` lists them. */
export async function agents(page: Page) {
  return (await api<{ backends: Agent[] }>(page, 'hello', {})).backends
}

/** The agent `id`, which must be the sandbox's stand-in of that name: no real agent runs and no real account is touched. */
export async function standInAgent(page: Page, id: string) {
  const agent = (await agents(page)).find((b) => b.id === id)
  if (agent?.path !== join(standIns, id)) throw new Error(`The sandbox host doesn't run the stand-in ${id}: start it with \`npm run dev -- --cdp\`.`)
  return agent
}

/** The login profiles the host has for the agent `id`. */
export async function loginProfilesOf(page: Page, id: string) {
  return (await agents(page)).find((b) => b.id === id)?.loginProfiles ?? []
}

/** Opens the agent's sheet from Extensions. */
export async function openAgentSheet(page: Page, name: string) {
  await page.getByRole('button', { name: /Extensions/ }).click()
  await page.locator('.agent-grid').getByText(name, { exact: true }).click()
}

/**
 * Add account on the agent's sheet: its sign-in opens in a terminal, and leaving it signs nothing in. Returns the new
 * login profile's id, with the sheet showing its "New login profile" row; `made` hears the id as soon as the host has
 * it, so a test that fails later still removes it.
 */
export async function addAccount(page: Page, agent: Agent, made: (id: string) => void) {
  const before = new Set((await loginProfilesOf(page, agent.id)).map((p) => p.id))
  await openAgentSheet(page, agent.name)
  await page.getByRole('button', { name: /Add account/ }).click()
  await expect(page.locator('.market-screen-title', { hasText: `Sign in to ${agent.name}` })).toBeVisible()
  await expect(page.locator('.setup-terminal-surface')).toBeVisible()
  let id: string | undefined
  await expect.poll(async () => (id = (await loginProfilesOf(page, agent.id)).find((p) => !before.has(p.id))?.id)).toBeTruthy()
  if (!id) throw new Error('Add account made no login profile.')
  made(id)
  await page.locator('.market-screen-header').getByRole('button', { name: 'Back' }).click()
  await expect(page.locator('.login-profile', { hasText: 'New login profile' })).toContainText('Not signed in')
  return id
}

/** Sends `text` from the open chat's composer (not the box of a thread open in the inspector). */
export async function typeMessage(page: Page, text: string) {
  const box = page.locator('.composer:not(.work-thread-composer) textarea')
  await box.fill(text)
  await box.press('Enter')
}

/** Opens Settings from the account panel, on its General page. */
export async function openGeneralSettings(page: Page) {
  await page.locator('.sidebar-account').click()
  await page.locator('.account-panel').getByText('Settings', { exact: true }).click()
}

/** Settings › Voice chat. */
export async function openVoiceSettings(page: Page) {
  await openGeneralSettings(page)
  await page.locator('.settings-item', { hasText: 'Voice chat' }).click()
}

/** Picks `option` in the settings picker now showing `current`. */
export async function pick(page: Page, current: string, option: string) {
  await page.locator(`.choice-picker[title="${current}"]`).click()
  await page.getByRole('menuitemradio', { name: option, exact: true }).click()
  await expect(page.locator(`.choice-picker[title="${option}"]`)).toBeVisible()
}

type Host = {
  snapshot(): Promise<HostSnapshot>
  call(baseURL: string, token: string, method: string, body: unknown, timeoutMs: number): Promise<unknown>
}

/** POST /api/<method> on the sandbox host, through the bridge the renderer uses. */
export function api<T>(page: Page, method: string, body: unknown): Promise<T> {
  return page.evaluate(async ([method, body]) => {
    const host = (window as unknown as { codync: { host: Host } }).codync.host
    const local = (await host.snapshot()).local
    if (!local) throw new Error('The sandbox host has not answered yet.')
    return host.call(local.baseURL, local.token, method, body, 30_000)
  }, [method, body] as const) as Promise<T>
}
