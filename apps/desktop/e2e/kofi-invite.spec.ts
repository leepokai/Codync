import { test, expect } from '@playwright/test'
import { api, attach, setPref, steerBot, typeMessage } from './sandbox.ts'

// Support on Ko-fi isn't clicked here: it opens the real browser (the bridge can't be stubbed), so it's checked by hand.
test('the first reply to the first prompt carries the Ko-fi invitation, in a session without a star ask, until dismissed', async () => {
  const { browser, page } = await attach()
  const name = `Ko-fi invitation e2e ${Date.now()}`
  let botId: string | undefined
  try {
    await page.reload() // A star ask this window already answered would hold the card.
    botId = (await steerBot(page, name, 1)).id // Its turn ends after a second with a plain "Done".
    await setPref(page, 'promptsSent', 0)
    await setPref(page, 'kofiInvite', { kind: 'never' })
    await setPref(page, 'githubStarAsk', { kind: 'never' })
    await page.getByRole('button', { name: 'Not now' }).click()
    await expect(page.getByText('Star Codync on GitHub')).toBeHidden()

    const row = page.locator('.roster-row', { hasText: name })
    const reply = page.getByRole('log', { name: 'Messages' }).getByText(/Done/).first()
    const card = page.locator('.kofi-invite')
    await row.click()
    await typeMessage(page, 'first')
    await expect(reply).toBeVisible()
    expect(JSON.parse((await page.evaluate(() => localStorage.getItem('kofiInvite'))) ?? 'null')).toMatchObject({ kind: 'due', bot: botId })
    await expect(card).toBeHidden() // This session answered the star ask: the card waits for a later one.

    await page.reload()
    await row.click()
    await expect(card).toBeVisible()
    await expect(card.locator('xpath=../..')).toContainText('Done') // In the reply's own row.
    await card.getByRole('button', { name: 'Dismiss' }).click()
    await expect(card).toBeHidden()
    await expect.poll(() => page.evaluate(() => localStorage.getItem('kofiInvite'))).toBe('{"kind":"done"}')

    await page.reload()
    await row.click()
    await expect(reply).toBeVisible()
    await expect(card).toBeHidden()
  } finally {
    await setPref(page, 'kofiInvite', { kind: 'done' })
    await setPref(page, 'githubStarAsk', { kind: 'done' })
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
