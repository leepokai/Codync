import { chromium, type Browser, type Page } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { SANDBOX_PREFIX, sandboxName } from '../src/shared/dev-sandbox.ts'

// Playwright loads specs as CommonJS here (the package has no "type": "module").
const root = resolve(__dirname, '../../..')

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
  return { browser, page }
}
