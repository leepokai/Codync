import { test, expect } from '@playwright/test'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, typeMessage } from './sandbox.ts'

// Lists `review` and `compact` right after session/new, answers every prompt with `ok` (host/tests/fixtures/commands_agent.py).
// Playwright loads specs as CommonJS here, so `__dirname` is defined.
const agent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/commands_agent.py')

test('the / list shows the agent\'s own commands, and one goes to the agent as typed', async () => {
  const { browser, page } = await attach()
  const name = `Agent commands ${Date.now()}`
  const field = page.locator('.composer textarea')
  const rows = page.getByRole('listbox', { name: 'Commands' }).getByRole('option')
  let botId: string | undefined
  const { skill } = await api<{ skill: { id: string } }>(page, 'installSkill', { name: `Own skill ${Date.now()}`, description: 'A Codync skill', instructions: 'Say hi.' })
  try {
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(agent), connectors: [], skills: [skill.id], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    // The agent lists its commands once it has started for the bot.
    await typeMessage(page, 'hello')
    await expect(page.locator('.transcript')).toContainText('ok')

    await field.fill('/')
    await expect(rows.filter({ hasText: 'review' })).toContainText('Review the changes')
    await expect(rows.filter({ hasText: 'compact' })).toContainText('Summarize the conversation')
    // Headings set Codync's own commands apart from the agent's.
    const headings = page.locator('.composer-menu .menu-section')
    await expect(headings).toHaveText(['Codync', /\S/])
    await expect(headings.last()).not.toHaveText('Codync')
    await field.fill('/compact')
    await expect(headings).toHaveCount(1, { timeout: 1000 })
    await expect(headings).not.toHaveText('Codync')
    await field.fill('/rev')
    await expect(rows).toHaveCount(1)
    await field.press('Enter')
    await expect(field).toHaveValue('/review ')

    await field.pressSequentially('the last commit')
    await field.press('Enter')
    await expect.poll(async () => {
      const { entries } = await api<{ entries: { kind: string; data: { text?: string } }[] }>(page, 'history', { botId })
      return entries.filter((e) => e.kind === 'user').map((e) => e.data.text)
    }).toEqual(['hello', '/review the last commit'])
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await api(page, 'removeSkill', { id: skill.id }).catch(() => {})
    await browser.close()
  }
})
