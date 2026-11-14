import { test, expect } from '@playwright/test'
import { resolve } from 'node:path'
import { api, attach } from './sandbox.ts'

type Entry = { id: string; turn: number; kind: string; data: { decision?: boolean; status?: string; selected?: string; from?: string } }

test('choice buttons preserve numeric labels and show the same answer after a language switch', async ({}, testInfo) => {
  const { browser, page } = await attach()
  const previousLanguage = await page.evaluate(() => localStorage.getItem('appLanguage'))
  let botId: string | undefined
  try {
    await page.evaluate(() => localStorage.setItem('appLanguage', JSON.stringify('en')))
    await page.reload()
    const offline = page.getByRole('button', { name: 'Continue without an account', exact: false })
    await expect(page.locator('.sidebar-account').or(offline).first()).toBeVisible()
    if (await offline.isVisible()) await offline.click()
    await expect(page.locator('.sidebar-account')).toBeVisible()
    const fixture = resolve(__dirname, '../../../host/tests/fixtures/team_agent.py')
    const command = process.platform === 'win32'
      ? `python -X utf8 -u "${fixture}"`
      : `python3 -X utf8 -u '${fixture.replaceAll("'", "'\\''")}'`
    const name = `Choice e2e ${Date.now()}`
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command, connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'ASK One\nTwo\nThree\nFour\nFifth line matters|2|4|4' })
    const card = page.locator('.decision-card').first()
    await expect(card.locator('.decision-card-question')).toContainText('One')
    await expect(card.locator('button.decision-option')).toHaveCount(2)
    await expect(card.getByPlaceholder('Something else')).toBeVisible()
    await expect(card).toContainText('Suggested')
    await card.getByRole('button', { name: 'Expand', exact: true }).click()
    await expect(card.locator('.decision-card-question')).not.toHaveClass(/line-clamp/)
    await expect(card).toContainText('Fifth line matters')
    await card.screenshot({ path: testInfo.outputPath('decision-en-pending-expanded.png') })
    await card.getByRole('button', { name: '2', exact: true }).click()
    await expect(card.locator('button.decision-option')).toHaveCount(0)
    await expect(page.locator('.transcript')).toContainText('Asked:')
    const history = await api<{ entries: Entry[] }>(page, 'history', { botId })
    const decision = history.entries.find((e) => e.data.decision)
    expect(decision?.data.status).toBe('answered')
    expect(decision?.data.selected).toBe('2')
    const replies = history.entries.filter((e) => e.kind === 'user' && e.data.from === 'decision')
    expect(replies).toHaveLength(1)
    expect(replies[0].turn).toBe(decision?.turn)
    await api(page, 'send', { botId, text: `ASK Wait for the build?|Yes|${'x'.repeat(200)}` })
    const expiring = page.locator('.decision-card').last()
    await expect(expiring.locator('button.decision-option')).toHaveCount(2)
    // A 200-character option with no spaces wraps inside the card instead of widening it.
    expect(await expiring.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true)
    await api(page, 'stop', { botId })
    await expect(expiring.locator('button.decision-option')).toHaveCount(0)
    await expect(expiring).toContainText('No answer — the bot moved on')
    // The last row takes the user's own words: they answer the card and become the user's message.
    await api(page, 'send', { botId, text: 'ASK Which way?|Left|Right' })
    const own = page.locator('.decision-card').last()
    await expect(own.locator('button.decision-option')).toHaveCount(2)
    await own.getByPlaceholder('Something else').fill('Neither, go up')
    await page.keyboard.press('Enter')
    await expect(own.locator('button.decision-option')).toHaveCount(0)
    await expect(own).toContainText('Neither, go up')
    const after = await api<{ entries: Entry[] }>(page, 'history', { botId })
    const ownCard = after.entries.filter((e) => e.data.decision).at(-1)
    expect(ownCard?.data.status).toBe('answered')
    expect(ownCard?.data.selected).toBe('Neither, go up')
    expect(after.entries.filter((e) => e.kind === 'user' && e.data.from === 'decision')).toHaveLength(2)
    await page.evaluate(() => localStorage.setItem('appLanguage', JSON.stringify('zh')))
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(card).toContainText('2')
    await expect(page.locator('.decision-card').nth(1)).toContainText('没有回答，机器人已继续')
    await expect(card.locator('button.decision-option')).toHaveCount(0)
    await card.screenshot({ path: testInfo.outputPath('decision-zh-answered.png') })
  } finally {
    if (botId) {
      await api(page, 'stop', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await page.evaluate((previous) => {
      if (previous === null) localStorage.removeItem('appLanguage')
      else localStorage.setItem('appLanguage', previous)
    }, previousLanguage)
    await page.reload()
    await browser.close()
  }
})
