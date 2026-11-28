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

/** The pause mark's box, in CSS pixels, as the avatar actually rendered it. */
const markBox = (page: Page, name: string) =>
  page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })
    .locator('.avatar-paused').evaluate((el) => {
      const r = el.getBoundingClientRect()
      return { top: r.top, right: r.right, width: r.width, height: r.height }
    })

test('the crown and the pause mark follow the icon size, and lead + paused keeps its mark', async () => {
  const { browser, page } = await attach()
  // The sandbox has no saved account, so the welcome covers the window; mark the choice made,
  // the way the pause-badge and Bot Manager specs do, so the roster under test is never behind it.
  await setPref(page, 'macAccountOnboardingCompleted', true)
  await setPref(page, 'withoutAccount', true)
  if (await page.locator('.welcome').count()) await page.reload()
  await expect(page.locator('.welcome')).toHaveCount(0, { timeout: 15_000 })
  const run = Date.now()
  const lead = `Lead Pause ${run}`
  const plain = `Plain Pause ${run}`
  const made: string[] = []
  const row = (name: string) => page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })
  try {
    made.push(await createBot(page, { name: plain, backend: 'claude' }))
    made.push(await createBot(page, { name: lead, backend: 'claude' }))
    // A lead is picked by the user, never on creation (the host's own `setLead`, api::bots).
    await call(page, 'setLead', { botId: made[1] })
    await expect(row(plain)).toBeVisible()
    await expect(row(lead)).toBeVisible()
    await call(page, 'updateBot', { id: made[0], paused: true })
    await call(page, 'updateBot', { id: made[1], paused: true })
    await expect(row(plain).locator('.avatar-paused')).toBeVisible()
    await expect(row(lead).locator('.avatar-paused')).toBeVisible()

    // The defect: at the smallest icon the crown's clearance math shrank the mark under 8px and it
    // was skipped, so a bot that was lead AND paused stopped saying Paused on its face. Now the
    // mark keeps its size, and it must survive at the smallest icon the slider allows.
    for (const iconSize of [24, 44]) {
      await setPref(page, 'rosterIconSize', iconSize)
      await expect(row(lead).locator('.avatar-paused')).toBeVisible({ timeout: 10_000 })
      await expect(row(plain).locator('.avatar-paused')).toBeVisible()
      const box = await markBox(page, lead)
      // Readable at every size, and never shrunk to fit the crown (the old math gave 4px at 24).
      expect(box.width).toBeGreaterThanOrEqual(8.5)
      expect(box.height).toBeGreaterThanOrEqual(8.5)
      // The mark is not swallowed by the crown: it stays inside the avatar, not pushed out of it.
      const avatar = await row(lead).locator('.avatar-frame').evaluate((el) => el.getBoundingClientRect().toJSON())
      expect(box.top).toBeGreaterThanOrEqual(avatar.y - 1)
      expect(box.right).toBeLessThanOrEqual(avatar.x + avatar.width + 1)
    }

    // Resuming clears it, on both.
    await call(page, 'updateBot', { id: made[0], paused: false })
    await call(page, 'updateBot', { id: made[1], paused: false })
    await expect(row(plain).locator('.avatar-paused')).toHaveCount(0)
    await expect(row(lead).locator('.avatar-paused')).toHaveCount(0)
  } finally {
    // Put the icon size back to the app's default, as roster-icon-size.spec.ts does: the sandbox
    // keeps the choice on this device, so leaving it at 44 fails that spec's "the default is 30"
    // when the two share a sandbox.
    await setPref(page, 'rosterIconSize', 30).catch(() => {})
    for (const id of made.reverse()) await call(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  }
})
