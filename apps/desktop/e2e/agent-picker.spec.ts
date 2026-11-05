import { test, expect } from '@playwright/test'
import { api, attach, steerBot } from './sandbox.ts'

type Hello = { backends: { id: string; name: string }[] }

test('going from agents back to models leaves no agent rows behind', async () => {
  const { browser, page } = await attach()
  const name = `Agent picker e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // Clients key their agent lists by id, so the host lists each agent once.
    const { backends } = await api<Hello>(page, 'hello', {})
    const ids = backends.map((b) => b.id)
    expect(new Set(ids).size, ids.join(', ')).toBe(ids.length)
    expect(backends.filter((b) => b.name === 'Kiro CLI')).toHaveLength(1)

    botId = (await steerBot(page, name, 1)).id
    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: `${name}, view details` }).click()
    const picker = page.locator('.details-panel .agent-picker')
    await picker.locator('.agent-picker-summary').click()
    const items = picker.locator('.agent-picker-item')
    const search = picker.getByRole('textbox', { name: /^Search (agents|models)$/ })
    await expect(search).toHaveAttribute('aria-label', 'Search models')

    for (const agent of ['Kiro CLI (not installed)', 'Custom command', 'Kiro CLI (not installed)']) {
      await picker.locator('.agent-picker-choice').first().click()
      await expect(search).toHaveAttribute('aria-label', 'Search agents')
      await items.filter({ hasText: agent }).first().click()
      await expect(search).toHaveAttribute('aria-label', 'Search models')
      await expect(items.filter({ hasText: /\(not installed\)|^Custom command$/ })).toHaveCount(0)
    }
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
