import { test, expect } from '@playwright/test'
import { join, resolve } from 'node:path'
import { api, attach, setPref, typeMessage } from './sandbox.ts'

// A scripted agent that sets a 30-minute follow-up when its message carries FOLLOW_UP_HERE[note]
// (host/tests/fixtures/routine_agent.py). Playwright loads specs as CommonJS here, so `__dirname` is defined.
const agent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/routine_agent.py')

test('a follow-up a bot sets is a line in its chat, translated around the note in 繁體中文', async () => {
  const { browser, page } = await attach()
  const name = `Follow-up e2e ${Date.now()}`
  let botId: string | undefined
  try {
    await setPref(page, 'appLanguage', 'en')
    await page.reload()
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: `python3 -X utf8 -u '${agent}'`, connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await typeMessage(page, 'FOLLOW_UP_HERE[Look at the build]')
    const transcript = page.locator('.transcript')
    await expect(transcript).toContainText('Follow-up in 30 minutes: Look at the build')
    await expect(transcript).toContainText('Routine completed successfully.')

    await setPref(page, 'appLanguage', 'zh-Hant')
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(transcript).toContainText('30 分鐘後跟進：Look at the build')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await page.reload()
    await browser.close()
  }
})
