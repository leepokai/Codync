import { test, expect } from '@playwright/test'
import { api, attach, setPref, steerBot, typeMessage } from './sandbox.ts'

test('an installation past setup is invited to star, then once more at the fifth prompt', async () => {
  const { browser, page } = await attach()
  const name = `Star invitation e2e ${Date.now()}`
  let botId: string | undefined
  try {
    await page.reload() // A sheet this window already closed would hold off: the count starts over here.
    const bot = await steerBot(page, name, 120)
    botId = bot.id
    await setPref(page, 'promptsSent', 3)
    await setPref(page, 'githubStarAsk', { kind: 'never' })
    const first = page.getByText('Star Codync on GitHub')
    await expect(first).toBeVisible()
    await page.getByRole('button', { name: 'Not now' }).click()
    await expect(first).toBeHidden()

    await page.locator('.roster-row', { hasText: name }).click()
    const second = page.getByText('Enjoying Codync?')

    await typeMessage(page, 'fourth')
    await expect.poll(() => page.evaluate(() => localStorage.getItem('promptsSent'))).toBe('4')
    await expect(second).toBeHidden()

    await typeMessage(page, 'fifth')
    await expect(second).toBeVisible()
    await page.getByRole('button', { name: 'Not now' }).click()
    await expect(second).toBeHidden()
    expect(await page.evaluate(() => localStorage.getItem('githubStarAsk'))).toBe('{"kind":"done"}')

    await typeMessage(page, 'sixth')
    await expect.poll(() => page.evaluate(() => localStorage.getItem('promptsSent'))).toBe('6')
    await expect(second).toBeHidden()
  } finally {
    await setPref(page, 'githubStarAsk', { kind: 'done' })
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
