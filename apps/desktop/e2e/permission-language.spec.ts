import { test, expect } from '@playwright/test'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, typeMessage } from './sandbox.ts'

// A scripted agent that asks before editing a.txt, then replies with the answer it got (host/tests/fake_agent.py).
const agent = join(resolve(__dirname, '../../..'), 'host/tests/fake_agent.py')

test('an approval card reads in the app language, its answers and outcome too', async () => {
  const { browser, page } = await attach()
  let botId: string | undefined
  try {
    await page.evaluate(() => localStorage.setItem('appLanguage', JSON.stringify('zh')))
    await page.reload()
    const name = `Approval language e2e ${Date.now()}`
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(agent), connectors: [], skills: [], notify: false, permission: 'ask',
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await typeMessage(page, 'go')

    const card = page.locator('.permission-card')
    await expect(card).toContainText('想要修改文件')
    await expect(card).toContainText('Edit a.txt') // what the agent wrote stays as it wrote it
    // The answers as numbered rows: allow before deny, once before always.
    await expect(card.locator('.approval-answer .decision-option-label')).toHaveText(['允许一次', '始终允许', '拒绝'])

    await card.getByRole('button', { name: '拒绝' }).click()
    // Answered, the card folds to its quiet line, how it ended at the end.
    await expect(page.locator('.request-line').last()).toContainText('已拒绝')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await page.reload()
    await browser.close()
  }
})
