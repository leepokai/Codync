import { test, expect, type Page } from '@playwright/test'
import type { HostSnapshot } from '../src/shared/ipc.ts'
import { attach, setPref } from './sandbox.ts'

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

test('a paused bot wears the pause mark on its avatar until it is resumed, and a group never does', async () => {
  const { browser, page } = await attach()
  // A sandbox with no saved account covers the window with the welcome; mark the choice made,
  // the way the Bot Manager's spec does, so the roster under test is never behind it.
  await setPref(page, 'macAccountOnboardingCompleted', true)
  await setPref(page, 'withoutAccount', true)
  if (await page.locator('.welcome').count()) await page.reload()
  await expect(page.locator('.welcome')).toHaveCount(0, { timeout: 15_000 })
  const run = Date.now()
  const bot = `Pause Mark ${run}`
  const group = `Pause Group ${run}`
  const made: string[] = []
  const row = (name: string) => page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })
  try {
    made.push(await createBot(page, { name: bot, backend: 'claude' }))
    made.push(await createBot(page, { name: group, kind: 'group', members: [...made] }))
    // The absence checks need the rows on screen first: count 0 is also true before mount.
    await expect(row(bot)).toBeVisible()
    await expect(row(group)).toBeVisible()
    await expect(row(bot).locator('.avatar-paused')).toHaveCount(0)

    // The same minimal patch the roster's Pause bot menu sends (lead-menus → store.setPaused).
    await call(page, 'updateBot', { id: made[0], paused: true })
    await expect(row(bot).locator('.avatar-paused')).toBeVisible()
    // Announced once: the row's own label names Paused, so its avatar's mark stays decorative. (Where the roster
    // mixes agents the label carries the agent's name between the two.)
    await expect(row(bot).locator('.avatar-paused')).toHaveAttribute('aria-hidden', 'true')
    const label = await row(bot).getAttribute('aria-label')
    expect(label?.startsWith(`${bot},`)).toBe(true)
    expect(label?.endsWith(', Paused')).toBe(true)
    // A group's face stays bare: groups carry no pause state of their own.
    await expect(row(group).locator('.avatar-paused')).toHaveCount(0)

    await call(page, 'updateBot', { id: made[0], paused: false })
    await expect(row(bot).locator('.avatar-paused')).toHaveCount(0)
  } finally {
    for (const id of made.reverse()) await call(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  }
})
