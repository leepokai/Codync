import { test, expect, type Page } from '@playwright/test'
import { join, resolve } from 'node:path'
import { api, attach, typeMessage } from './sandbox.ts'

// A scripted agent that answers `initialize` the way Claude's adapter does and reports the context use a message names
// (host/tests/fixtures/instructions_agent.py). Playwright loads specs as CommonJS here, so `__dirname` is defined.
const agent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/instructions_agent.py')

type Bot = { id: string; compaction: unknown }

/** The bot's Summarize long chats row, on the Settings tab of the details beside its chat. */
async function summarizeRow(page: Page, name: string) {
  await page.locator('.roster-row', { hasText: name }).click()
  const details = page.locator('.details-panel')
  if (!(await details.isVisible())) await page.getByRole('button', { name: `${name}, view details` }).click()
  await details.getByRole('tab', { name: 'Settings' }).click()
  return details.locator('.value-row', { hasText: 'Summarize long chats' })
}

const menuRow = (page: Page, title: string | RegExp) => page.locator('.menu-row', { hasText: title })

/** The setting as the host keeps it: an update that changes nothing answers with the bot. */
const stored = async (page: Page, id: string) => (await api<{ bot: Bot }>(page, 'updateBot', { id })).bot.compaction

test('before its first reply a Claude Code bot offers only the three choices', async () => {
  const { browser, page } = await attach()
  const name = `Summarize e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await api<{ bot: Bot }>(page, 'createBot', { name, backend: 'claude', connectors: [], skills: [], notify: false })).bot.id
    const pill = (await summarizeRow(page, name)).locator('.choice-picker')
    await expect(pill).toHaveText('Early (default)')
    await pill.click()
    for (const title of ['Early (default)', 'Near the limit', 'Only when full']) await expect(menuRow(page, title)).toBeEnabled()
    await expect(menuRow(page, "Sizes show after the bot's next reply")).toBeDisabled()
    await expect(menuRow(page, 'Set a size')).toHaveCount(0)
    await menuRow(page, 'Only when full').click()
    await expect(pill).toHaveText('Only when full')
    await expect.poll(() => stored(page, botId!)).toBe('off')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test('once the agent tells its window, Set a size lists what it holds and the size picked is kept', async () => {
  const { browser, page } = await attach()
  const name = `Summarize sizes e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const command = `python3 -X utf8 -u '${agent}' claude`
    botId = (await api<{ bot: Bot }>(page, 'createBot', { name, backend: 'custom', command, connectors: [], skills: [], notify: false })).bot.id
    const row = await summarizeRow(page, name)
    await expect(row, 'a custom command shows it once it runs Claude').toHaveCount(0)

    await typeMessage(page, 'USAGE 10000 SIZE 1000000')
    const pill = row.locator('.choice-picker')
    await expect(pill).toHaveText('Early (default)', { timeout: 20_000 })
    await pill.click()
    await menuRow(page, 'Set a size').click()
    await expect(menuRow(page, /^50K$/)).toBeVisible()
    await expect(menuRow(page, /^960K$/)).toBeAttached()
    await menuRow(page, /^120K$/).click()
    await expect(pill).toHaveText('120K')
    await expect.poll(() => stored(page, botId!)).toBe(120_000)

    // The submenu opens at the size picked, and its back row returns to the choices.
    await pill.click()
    await menuRow(page, 'Set a size').click()
    await expect(menuRow(page, /^120K$/)).toHaveAttribute('aria-checked', 'true')
    await menuRow(page, 'Set a size').click()
    await menuRow(page, 'Near the limit').click()
    await expect(pill).toHaveText('Near the limit')
    await expect.poll(() => stored(page, botId!)).toBe('nearLimit')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test('a bot on an agent Codync does not summarize for has no Summarize long chats row', async () => {
  const { browser, page } = await attach()
  const name = `Summarize gemini e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = (await api<{ bot: Bot }>(page, 'createBot', { name, backend: 'gemini', connectors: [], skills: [], notify: false })).bot
    botId = bot.id
    expect(bot.compaction).toBeNull()
    const row = await summarizeRow(page, name)
    await expect(page.locator('.details-panel .value-row', { hasText: 'Workspace' })).toBeVisible()
    await expect(row).toHaveCount(0)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
