import { test, expect } from '@playwright/test'
import { join, resolve } from 'node:path'
import { api, attach, typeMessage } from './sandbox.ts'

// A scripted agent that asks before editing a.txt, then replies with the answer it got (host/tests/fake_agent.py).
const agent = join(resolve(__dirname, '../../..'), 'host/tests/fake_agent.py')
const python = process.platform === 'win32' ? `python -X utf8 -u "${agent}"` : `python3 -X utf8 -u '${agent}'`

test('an approval card reads in the app language, its answers and outcome too', async () => {
  const { browser, page } = await attach()
  let botId: string | undefined
  try {
    await page.evaluate(() => localStorage.setItem('appLanguage', JSON.stringify('zh')))
    await page.reload()
    const name = `Approval language e2e ${Date.now()}`
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: python, connectors: [], skills: [], notify: false, permission: 'ask',
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await typeMessage(page, 'go')

    const card = page.locator('.permission-card')
    await expect(card).toContainText('想要修改文件')
    await expect(card).toContainText('Edit a.txt') // what the agent wrote stays as it wrote it
    const answers = card.locator('.permission-option-label')
    await expect(answers).toHaveText(['允许一次', '始终允许', '拒绝'])

    await card.locator('.permission-option', { hasText: '拒绝' }).click()
    await expect(card).toContainText('已拒绝')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await page.reload()
    await browser.close()
  }
})
