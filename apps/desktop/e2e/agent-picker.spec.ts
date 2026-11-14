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

test('the agent list blurs under an edge only while there is more past it', async () => {
  const { browser, page } = await attach()
  const name = `Agent picker edges e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await steerBot(page, name, 1)).id
    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: `${name}, view details` }).click()
    const picker = page.locator('.details-panel .agent-picker')
    await picker.locator('.agent-picker-summary').click()
    await picker.locator('.agent-picker-choice').first().click()
    const list = picker.locator('.agent-picker-list')
    expect(await list.evaluate((el) => el.scrollHeight - el.clientHeight)).toBeGreaterThan(48)

    const edges = () => list.evaluate((el) => [getComputedStyle(el, '::before').opacity, getComputedStyle(el, '::after').opacity])
    for (const [at, shown] of [[0, ['0', '1']], [0.5, ['1', '1']], [1, ['1', '0']]] as const) {
      await list.evaluate((el, at) => { el.scrollTop = (el.scrollHeight - el.clientHeight) * at }, at)
      await expect.poll(edges, { message: `scrolled to ${at}` }).toEqual(shown)
    }

    // Nothing found: the caption alone, with no list to scroll or blur it.
    await picker.getByRole('textbox', { name: 'Search agents' }).fill('no such agent')
    await expect(picker.getByText('No results')).toBeVisible()
    await expect(list).toHaveCount(0)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
