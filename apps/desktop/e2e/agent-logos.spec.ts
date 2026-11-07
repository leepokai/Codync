import { test, expect, type Page } from '@playwright/test'
import type { HostSnapshot } from '../src/shared/ipc.ts'
import { attach, openGeneralSettings } from './sandbox.ts'

type Bridge = {
  codync: {
    host: {
      snapshot(): Promise<HostSnapshot>
      call(baseURL: string, token: string, method: string, body: unknown, timeoutMs: number): Promise<unknown>
    }
  }
}

/** A method on the sandbox's own host, through the bridge the renderer uses. */
function call<T>(page: Page, method: string, body: unknown): Promise<T> {
  return page.evaluate(async ({ method, body }) => {
    const { host } = (window as unknown as Bridge).codync
    const { local } = await host.snapshot()
    if (!local) throw new Error('The sandbox host is not running.')
    return host.call(local.baseURL, local.token, method, body, 10_000)
  }, { method, body }) as Promise<T>
}

const createBot = async (page: Page, body: object) => (await call<{ bot: { id: string } }>(page, 'createBot', body)).bot.id

test('bots on different agents show their logos until Show agent logos is turned off', async () => {
  const { browser, page } = await attach()
  const run = Date.now()
  const claude = `Logo Claude ${run}`
  const codex = `Logo Codex ${run}`
  const group = `Logo Group ${run}`
  const made: string[] = []
  const row = (name: string) => page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })
  const logos = page.locator('[data-roster-row] .avatar-agent')
  const toggle = page.getByRole('switch').filter({ hasText: 'Show agent logos' })
  try {
    made.push(await createBot(page, { name: claude, backend: 'claude' }), await createBot(page, { name: codex, backend: 'codex' }))
    made.push(await createBot(page, { name: group, kind: 'group', members: [...made] }))

    await expect(row(claude).locator('.avatar-agent')).toBeVisible()
    await expect(row(codex).locator('.avatar-agent')).toBeVisible()
    await expect(row(group).locator('.avatar-agent')).toHaveCount(0)
    // Screen readers hear the agent with the bot's name.
    await expect(row(codex)).toHaveAttribute('aria-label', new RegExp(`^${codex}, Codex`))

    await openGeneralSettings(page)
    await expect(toggle).toHaveAttribute('aria-checked', 'true')
    await toggle.click()
    await expect(toggle).toHaveAttribute('aria-checked', 'false')
    await expect(logos).toHaveCount(0)
    await expect(row(codex)).toHaveAttribute('aria-label', new RegExp(`^${codex}(, on |$)`))

    // The choice is kept on this device.
    await page.reload()
    await expect(row(codex)).toBeVisible()
    await openGeneralSettings(page)
    await expect(toggle).toHaveAttribute('aria-checked', 'false')
    await expect(logos).toHaveCount(0)

    await toggle.click()
    await expect(row(claude).locator('.avatar-agent')).toBeVisible()
    await expect(row(codex).locator('.avatar-agent')).toBeVisible()
  } finally {
    for (const id of made.reverse()) await call(page, 'deleteBot', { botId: id }).catch(() => {})
    // Back to the default, in this window's copy too.
    await page.evaluate(() => localStorage.removeItem('showAgentLogos'))
    await page.reload()
    await browser.close()
  }
})
