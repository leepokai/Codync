import { test, expect } from '@playwright/test'
import { api, attach, echoBot, openPage, setPref, sidePanel, withPrefs } from './sandbox.ts'

// Browser and Terminal open a new tab each time they are picked; each terminal is its own shell, ended with its tab.
test('picking Browser or Terminal again opens another tab', async () => {
  test.setTimeout(120_000)
  const { browser, page } = await attach()
  await setPref(page, 'macAccountOnboardingCompleted', true)
  await setPref(page, 'withoutAccount', true)
  const restore = await withPrefs(page, { listHidden: false, panelLayout: [{ tabs: [], shown: null }] })
  const name = `Multi tabs ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await echoBot(page, name)).id
    await page.locator('.roster-row', { hasText: name }).first().click()
    await page.locator('.title-pill').click()
    const panel = page.locator('.side-panel .details-panel')
    const tabs = panel.locator('.panel-tabs-row').first().getByRole('tab')
    const pick = async (view: string) => {
      await panel.getByRole('button', { name: 'Open a view', exact: true }).first().click()
      await panel.locator('.panel-choice', { hasText: view }).click()
    }
    await pick('Terminal')
    await expect(panel.locator('.terminal-tab .xterm')).toBeVisible({ timeout: 15_000 })
    await page.keyboard.type('echo first-shell\n')
    await expect(panel.locator('.terminal-tab')).toContainText('first-shell', { timeout: 15_000 })
    await pick('Terminal')
    await pick('Browser')
    await pick('Browser')
    await expect(tabs).toHaveText(['Terminal', 'Terminal 2', 'Browser', 'Browser 2'])
    // The second terminal is a shell of its own.
    await tabs.nth(1).click()
    await expect(panel.locator('.terminal-tab .xterm')).toBeVisible({ timeout: 15_000 })
    await expect(panel.locator('.terminal-tab')).not.toContainText('first-shell')
    // The first one is still as it was.
    await tabs.nth(0).click()
    await expect(panel.locator('.terminal-tab')).toContainText('first-shell', { timeout: 15_000 })
    // Each browser keeps its own address.
    await tabs.nth(2).click()
    await panel.getByLabel('Web address').fill('example.com')
    await panel.getByLabel('Web address').press('Enter')
    await tabs.nth(3).click()
    await expect(panel.getByLabel('Web address')).toHaveValue('')
    await tabs.nth(2).click()
    await expect(panel.getByLabel('Web address')).toHaveValue('https://example.com/')
    // Closing a terminal's tab ends its shell.
    const [, second] = (await page.evaluate(() => JSON.parse(localStorage.getItem('panelLayout') ?? '[]')) as { tabs: string[] }[])[0]!.tabs
    const { term } = await api<{ term: string }>(page, 'botTerminal', { botId, tab: second })
    await panel.getByRole('button', { name: 'Close Terminal 2', exact: true }).click()
    await expect(tabs).toHaveText(['Terminal', 'Browser', 'Browser 2'])
    await expect.poll(() => api(page, 'termInput', { term, data: btoa('x') }).then(() => 'open', () => 'gone')).toBe('gone')
  } finally {
    await restore()
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test("a thread without a bot opens a new terminal each time too", async () => {
  test.setTimeout(90_000)
  const { browser, page } = await attach()
  try {
    await openPage(page, 'Threads')
    await page.locator('.sidebar-header').getByRole('button', { name: 'New thread', exact: true }).click()
    const start = page.locator('.new-thread')
    const bots = start.locator('.thread-option[title="Choose a bot"]')
    if (!(await bots.textContent())?.includes('No bot')) {
      await bots.click()
      await page.getByRole('menuitemradio', { name: 'No bot', exact: true }).click()
    }
    await start.getByRole('textbox').fill(`Two shells ${Date.now()}`)
    await start.getByRole('textbox').press('Enter')
    await expect(page.locator('.thread-chat')).toBeVisible({ timeout: 20_000 })
    await page.getByRole('button', { name: 'Conversation details', exact: true }).click()
    const panel = sidePanel(page)
    const tabs = panel.locator('.panel-tabs-row').getByRole('tab')
    for (let i = 0; i < 2; i++) {
      await panel.getByRole('button', { name: 'Open a view', exact: true }).click()
      await panel.locator('.panel-choice', { hasText: 'Terminal' }).click()
    }
    await expect(tabs).toHaveText(['Terminal', 'Terminal 2'])
    await expect(panel.locator('.terminal-tab .xterm')).toBeVisible({ timeout: 15_000 })
    await panel.getByRole('button', { name: 'Close Terminal 2', exact: true }).click()
    await expect(tabs).toHaveText(['Terminal'])
  } finally {
    await openPage(page, 'Bots').catch(() => {})
    await browser.close()
  }
})
