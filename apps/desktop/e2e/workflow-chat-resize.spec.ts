import { test, expect, type Locator, type Page } from '@playwright/test'
import { api, attach, openPage, setPref, workBot } from './sandbox.ts'

// The window's list at its default width, shown, so the page beside it is as wide as in an ordinary window.
const LAYOUT = { desktopSidebarWidth: 266, listHidden: false, detailsPanelWidth: 360 }

/** Sets `prefs` for the test; the function it returns puts back what the sandbox had. */
async function withPrefs(page: Page, prefs: Record<string, unknown>) {
  const keys = Object.keys(prefs)
  const saved = await page.evaluate((keys) => keys.map((key) => localStorage.getItem(key)), keys)
  for (const key of keys) await setPref(page, key, prefs[key])
  return async () => {
    for (const [i, key] of keys.entries()) {
      await page.evaluate(([key, value]) => {
        if (value === null) localStorage.removeItem(key)
        else localStorage.setItem(key, value)
        window.dispatchEvent(new StorageEvent('storage', { key, newValue: value }))
      }, [key, saved[i] ?? null] as const).catch(() => {})
    }
  }
}

/** Drags the side panel's edge both ways: wider, then narrower than it started. */
async function dragBothWays(panel: Locator) {
  const page = panel.page()
  const shown = async () => (await panel.boundingBox())!.width
  // Once it has slid in, flush with the window's edge, and stopped moving.
  const edge = await page.evaluate(() => innerWidth)
  await expect.poll(async () => { const box = (await panel.boundingBox())!; return Math.round(box.x + box.width) }).toBe(edge)
  const settled = async () => { let last = -1; await expect.poll(async () => { const now = await shown(); const same = now === last; last = now; return same }).toBe(true) }
  const drag = async (dx: number) => {
    const handle = (await panel.locator('.resize-handle').boundingBox())!
    await page.mouse.move(handle.x + 1, handle.y + 200)
    await page.mouse.down()
    await page.mouse.move(handle.x + 1 + dx, handle.y + 200, { steps: 8 })
    await page.mouse.up()
    await settled()
  }
  const start = await shown()
  await drag(-60)
  expect(await shown()).toBeGreaterThan(start + 40)
  await drag(400)
  expect(await shown()).toBeLessThan(start - 40)
}

test('the chat beside a workflow drags wider and narrower in a window of ordinary width, and covers a canvas too narrow', async () => {
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  type Listed = { workflows: { id: string }[] }
  const ids = async () => (await api<Listed>(page, 'workflows', {})).workflows.map((w) => w.id)
  const before = new Set(await ids())
  const restore = await withPrefs(page, { ...LAYOUT, workflowChatOpen: true })
  try {
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1100, height: 760, deviceScaleFactor: 1, mobile: false })
    await page.locator('.nav-rail [data-page="workflows"]').click()
    await page.locator('.sidebar-header').getByRole('button', { name: 'New workflow', exact: true }).click()
    const panel = page.locator('.side-panel')
    await expect(panel).toBeVisible()
    await dragBothWays(panel)

    // A page too narrow for the chat beside the canvas: the chat covers it, with no edge to drag.
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 900, height: 760, deviceScaleFactor: 1, mobile: false })
    await expect.poll(async () => (await panel.boundingBox())!.width).toBe((await page.locator('.workflows-page').boundingBox())!.width)
    await expect(panel.locator('.resize-handle')).toHaveCount(0)
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride').catch(() => {})
    await restore()
    for (const id of await ids().catch(() => [])) if (!before.has(id)) await api(page, 'deleteWorkflow', { id }).catch(() => {})
    await browser.close()
  }
})

test("a thread's side panel drags wider and narrower in a window of ordinary width", async () => {
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  const restore = await withPrefs(page, LAYOUT)
  const title = 'Resize the panel'
  let botId: string | undefined
  try {
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1100, height: 760, deviceScaleFactor: 1, mobile: false })
    botId = (await workBot(page, `Panel resize e2e ${Date.now()}`)).id
    // A task whose thread works until it is stopped (host/tests/fixtures/work_agent.py).
    await api(page, 'send', { botId, text: `ADD ${title}|HOLD:e2e` })
    await expect.poll(async () => (await api<{ bot: { work: { title: string }[] } }>(page, 'updateBot', { id: botId })).bot.work.some((t) => t.title === title)).toBe(true)
    await openPage(page, 'Threads')
    await page.locator(`[data-thread-row="${title}"]`).click()
    await page.getByRole('button', { name: 'Conversation details', exact: true }).click()
    const panel = page.locator('.side-panel')
    await expect(panel).toBeVisible()
    await dragBothWays(panel)
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride').catch(() => {})
    await restore()
    if (botId) {
      await api(page, 'stopWork', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})
