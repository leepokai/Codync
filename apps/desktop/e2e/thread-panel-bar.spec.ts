import { test, expect, type Page } from '@playwright/test'
import { api, attach, openPage, setPref, workBot } from './sandbox.ts'

/** Sets the prefs for the test; what it returns puts back the sandbox's. */
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

/** The buttons at the window bar's end, by name, in order. */
const barButtons = (page: Page) => page.locator('.window-bar-end button').evaluateAll((all) => all.map((b) => b.getAttribute('aria-label') ?? b.getAttribute('title')))

test("a thread's panel puts its buttons in the window's bar as a bot's chat does, its tabs row only tabs and +", async ({}, info) => {
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  const name = `Thread bar e2e ${Date.now()}`
  const title = 'Header like the bot'
  let botId: string | undefined
  try {
    await setPref(page, 'listHidden', false)
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1300, height: 760, deviceScaleFactor: 2, mobile: false })
    botId = (await workBot(page, name)).id
    // A task whose thread works until it is stopped (host/tests/fixtures/work_agent.py).
    await api(page, 'send', { botId, text: `ADD ${title}|HOLD:e2e` })
    await expect.poll(async () => (await api<{ bot: { work: { title: string }[] } }>(page, 'updateBot', { id: botId })).bot.work.some((t) => t.title === title)).toBe(true)

    // The bot's chat, panel open: the row the thread's must match.
    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: 'Conversation details', exact: true }).click()
    await expect(page.locator('.side-panel:not([inert])')).toBeVisible()
    const botBar = await barButtons(page)
    await page.screenshot({ path: info.outputPath('bot-panel.png') })

    await openPage(page, 'Threads')
    await page.locator(`[data-thread-row="${title}"]`).click()
    // Closed, only the toggle; the thread's status stays beside its title.
    await expect.poll(() => barButtons(page)).toEqual(['Conversation details'])
    await expect(page.locator('.window-bar-title .window-bar-status')).toContainText('Working')
    await page.getByRole('button', { name: 'Conversation details', exact: true }).click()
    const panel = page.locator('.side-panel:not([inert])')
    await expect(panel).toBeVisible()
    await expect.poll(() => barButtons(page)).toEqual(botBar)
    expect(botBar).toEqual(['Create template', 'Maximize panel', 'Split panel', 'Conversation details'])
    // The tabs row has the tabs and + only.
    const tabs = panel.locator('.panel-tabs').first()
    await expect(tabs.getByRole('button', { name: 'Open a view', exact: true })).toBeVisible()
    for (const gone of ['Create template', 'Split panel', 'Close details']) await expect(tabs.getByRole('button', { name: gone, exact: true })).toHaveCount(0)
    await page.screenshot({ path: info.outputPath('thread-panel.png') })

    // Maximize covers the thread, and puts it back.
    const view = page.locator('.thread-chat')
    await page.getByRole('button', { name: 'Maximize panel', exact: true }).click()
    await expect.poll(async () => (await panel.boundingBox())!.width).toBe((await view.boundingBox())!.width)
    await page.getByRole('button', { name: 'Maximize panel', exact: true }).click()
    await expect.poll(async () => (await panel.boundingBox())!.width).toBeLessThan((await view.boundingBox())!.width)

    // Too narrow for the panel beside it: it covers the thread already, so no Maximize.
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 760, height: 760, deviceScaleFactor: 2, mobile: false })
    await expect.poll(() => barButtons(page)).toEqual(['Create template', 'Split panel', 'Conversation details'])
    await page.screenshot({ path: info.outputPath('thread-panel-narrow.png') })
    await page.getByRole('button', { name: 'Conversation details', exact: true }).click()
    await expect.poll(() => barButtons(page)).toEqual(['Conversation details'])
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride').catch(() => {})
    if (botId) {
      await api(page, 'stopWork', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})

test("a panel's tabs share its width, narrow ones hide their close button, and + stays in sight", async ({}, info) => {
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  const name = `Panel tabs e2e ${Date.now()}`
  const title = 'Many tabs'
  const tabs = ['details', 'library', 'memory', 'computer', 'browser', 'settings', 'terminal']
  const restore = await withPrefs(page, { listHidden: false, panelLayout: [{ tabs, shown: 'details' }] })
  let botId: string | undefined
  try {
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1400, height: 760, deviceScaleFactor: 2, mobile: false })
    botId = (await workBot(page, name)).id
    await api(page, 'send', { botId, text: `ADD ${title}|HOLD:e2e` })
    await expect.poll(async () => (await api<{ bot: { work: { title: string }[] } }>(page, 'updateBot', { id: botId })).bot.work.some((t) => t.title === title)).toBe(true)
    const check = async (where: string) => {
      const panel = page.locator('.side-panel:not([inert])')
      const row = panel.locator('.panel-tabs-row').first()
      await expect(row.getByRole('tab')).toHaveCount(tabs.length)
      for (const width of [300, 440, 620]) {
        await setPref(page, 'detailsPanelWidth', width)
        await expect.poll(async () => Math.round((await panel.boundingBox())!.width)).toBe(width + 1)
        const box = (await panel.boundingBox())!
        const plus = (await panel.getByRole('button', { name: 'Open a view', exact: true }).first().boundingBox())!
        expect(plus.x + plus.width).toBeLessThanOrEqual(box.x + box.width)
        const widths = await row.locator('.panel-tab').evaluateAll((all) => all.map((t) => t.getBoundingClientRect().width))
        expect(Math.min(...widths)).toBeGreaterThanOrEqual(55)
        // Each as wide as the others.
        expect(Math.max(...widths) - Math.min(...widths)).toBeLessThan(1)
        // Wide enough for all, none scrolls out.
        if (width === 620) expect(await row.evaluate((r) => r.scrollWidth <= r.clientWidth)).toBe(true)
        await page.screenshot({ path: info.outputPath(`${where}-tabs-${width}.png`) })
      }
    }
    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: 'Conversation details', exact: true }).click()
    await check('bot')
    await openPage(page, 'Threads')
    await page.locator(`[data-thread-row="${title}"]`).click()
    await page.getByRole('button', { name: 'Conversation details', exact: true }).click()
    await check('thread')
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride').catch(() => {})
    await restore()
    await setPref(page, 'detailsPanelWidth', 400).catch(() => {})
    if (botId) {
      await api(page, 'stopWork', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})
