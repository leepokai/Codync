import { test, expect, type Page } from '@playwright/test'
import type { HostSnapshot } from '../src/shared/ipc.ts'
import { attach, openGeneralSettings, openPage, setPref } from './sandbox.ts'

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

test("the Bots list's icons follow the Bot icon size setting, and Settings' slider reads and writes it", async () => {
  const { browser, page } = await attach()
  // A sandbox with no saved account covers the window with the welcome; mark the choice made,
  // the way the Bot Manager's spec does, so the roster under test is never behind it.
  await setPref(page, 'macAccountOnboardingCompleted', true)
  await setPref(page, 'withoutAccount', true)
  if (await page.locator('.welcome').count()) await page.reload()
  await expect(page.locator('.welcome')).toHaveCount(0, { timeout: 15_000 })
  const run = Date.now()
  const bot = `Icon Size ${run}`
  const made: string[] = []
  const row = (name: string) => page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })
  const frame = () => row(bot).locator('.avatar-frame')
  try {
    made.push(await createBot(page, { name: bot, backend: 'claude' }))
    // The app's default: 30.
    await expect(frame()).toHaveCSS('width', '30px')

    // A preference written as another window would write it arrives live.
    await setPref(page, 'rosterIconSize', 44)
    await expect(frame()).toHaveCSS('width', '44px')
    // The choice is kept on this device.
    await page.reload()
    await expect(frame()).toHaveCSS('width', '44px')

    // Settings reads the stored size, and its slider writes it back.
    await openGeneralSettings(page)
    const slider = page.getByLabel('Bot icon size')
    await expect(slider).toHaveValue('44')
    await slider.focus()
    await slider.press('ArrowLeft')
    await expect(slider).toHaveValue('42')
    await openPage(page, 'Bots')
    await expect(frame()).toHaveCSS('width', '42px')
  } finally {
    await setPref(page, 'rosterIconSize', 30).catch(() => {})
    for (const id of made.reverse()) await call(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  }
})
