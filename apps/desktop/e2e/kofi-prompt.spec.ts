import { test, expect, type Page } from '@playwright/test'
import { api, attach, setPref, steerBot, typeMessage } from './sandbox.ts'

const kofiAsk = async (page: Page) => JSON.parse((await page.evaluate(() => localStorage.getItem('kofiAsk'))) ?? 'null')

// A reload is the next opening of the window (the sandbox runs headless, so it never hides).
// Support on Ko-fi isn't clicked here: it opens the real browser (the bridge can't be stubbed), so it's checked by hand.
test('the Ko-fi invitation comes as a sheet at the next opening after the third active day, then waits for the thirtieth', async () => {
  const { browser, page } = await attach()
  const name = `Ko-fi invitation e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const sheet = page.getByRole('dialog', { name: 'Support Codync' })
    botId = (await steerBot(page, name, 1)).id
    await setPref(page, 'kofiAsk', { asked: 0, day: 0 })
    await setPref(page, 'activeDays', { count: 2, last: '2000-1-1' })
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    await typeMessage(page, 'the third active day')
    await expect(page.getByRole('log', { name: 'Messages' }).getByText(/Done/).first()).toBeVisible()
    expect(JSON.parse((await page.evaluate(() => localStorage.getItem('activeDays'))) ?? 'null')).toMatchObject({ count: 3 })
    await expect(sheet).toBeHidden() // Not in the opening that reached it, and never in the chat.

    await page.reload()
    await expect(sheet).toBeVisible()
    await expect(sheet).toContainText('ko-fi.com/leepokai')
    expect(await kofiAsk(page)).toEqual({ asked: 1, day: 3 })
    await sheet.getByRole('button', { name: 'Not now' }).click()
    await expect(sheet).toBeHidden()

    await page.reload()
    await expect(page.locator('.roster-row', { hasText: name })).toBeVisible()
    await expect(sheet).toBeHidden()

    await setPref(page, 'activeDays', { count: 30, last: '2000-1-1' })
    await page.reload()
    await expect(sheet).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(sheet).toBeHidden()
    expect(await kofiAsk(page)).toEqual({ asked: 2, day: 30 })
  } finally {
    await setPref(page, 'kofiAsk', { asked: 3, day: 0 })
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
