import { chromium, type Browser, type Page } from '@playwright/test'
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { SANDBOX_PREFIX, sandboxName } from '../src/shared/dev-sandbox.ts'
import type { HostSnapshot } from '../src/shared/ipc.ts'

// Playwright loads specs as CommonJS here (the package has no "type": "module").
const root = resolve(__dirname, '../../..')
// A scripted agent whose turn waits until a steered message joins it (host/tests/fixtures/steer_agent.py).
const steerAgent = join(root, 'host/tests/fixtures/steer_agent.py')

/** This worktree's sandbox: its name, host port and DevTools port (`npm run dev -- --cdp` opens port + 10000). */
export function sandbox() {
  const name = sandboxName(basename(root))
  const record = join(homedir(), SANDBOX_PREFIX + name, 'sandbox.json')
  let port: number
  try {
    port = (JSON.parse(readFileSync(record, 'utf8')) as { port: number }).port
  } catch {
    throw new Error(`No sandbox for ${root}: run \`npm run dev -- --cdp\` in apps/desktop first.`)
  }
  return { name, port, cdp: port + 10000 }
}

/** Freezes the sandbox host, still connected and answering nothing, until the function it returns. Not on Windows. */
export function freezeHost() {
  const pid = Number(execFileSync('lsof', ['-t', '-sTCP:LISTEN', `-iTCP:${sandbox().port}`], { encoding: 'utf8' }).trim())
  process.kill(pid, 'SIGSTOP')
  return () => process.kill(pid, 'SIGCONT')
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
  // So would the ask to add a computer in no account, on a signed-in `--cloud` sandbox.
  const notNow = page.getByRole('alertdialog').getByRole('button', { name: 'Not now' })
  if (await notNow.isVisible()) await notNow.click()
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
    name, backend: 'custom', command: `python3 -X utf8 -u '${steerAgent}' steer ${seconds}`, connectors: [], skills: [], notify: false,
  })
  return bot
}

/** Sends `text` from the open chat's composer. */
export async function typeMessage(page: Page, text: string) {
  await page.locator('.composer textarea').fill(text)
  await page.locator('.composer textarea').press('Enter')
}

/** Opens Settings from the account panel, on its General page. */
export async function openGeneralSettings(page: Page) {
  await page.locator('.sidebar-account').click()
  await page.locator('.account-panel').getByText('Settings', { exact: true }).click()
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
