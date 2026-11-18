import { test, expect } from '@playwright/test'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, echoBot } from './sandbox.ts'

// A scripted agent that asks before editing a.txt, then replies with the answer it got (host/tests/fake_agent.py).
const agent = join(resolve(__dirname, '../../..'), 'host/tests/fake_agent.py')

// Issue #366: a bot blocked on an approval card keeps showing that it waits on you in the sidebar,
// the card counted in its badge until answered, even after its chat was opened.
test('a bot waiting on an approval card keeps its badge until the card is answered', async () => {
  const { browser, page } = await attach()
  const ids: string[] = []
  try {
    const name = `Waiting e2e ${Date.now()}`
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(agent), connectors: [], skills: [], notify: false, permission: 'ask',
    })
    ids.push(bot.id)
    const other = `Elsewhere e2e ${Date.now()}`
    ids.push((await echoBot(page, other)).id)

    const row = page.locator('.roster-row', { hasText: name })
    // The card arrives while another chat is open: it shows on the row.
    await page.locator('.roster-row', { hasText: other }).click()
    await api(page, 'send', { botId: bot.id, text: 'go' })
    await expect(row).toContainText('Needs approval')
    await expect(row.locator('.unread-badge')).toHaveText('1')
    await expect(row.locator('.unread-badge')).toHaveClass(/waiting/)

    // Opened once and left unanswered, the card still waits, so the badge stays, read or not.
    await row.click()
    await expect(page.locator('.permission-card')).toBeVisible()
    await api(page, 'markRead', { botId: bot.id })
    await page.locator('.roster-row', { hasText: other }).click()
    await expect(row).toContainText('Needs approval')
    await expect(row.locator('.unread-badge')).toHaveText('1')

    // Folded away in a section, its heading carries the amber count and says why.
    const section = `Waiting section ${Date.now()}`
    await api(page, 'updateBot', { id: bot.id, section })
    const heading = page.locator(`.roster-section-toggle[title="${section}"]`)
    await heading.click()
    await expect(heading.locator('.unread-badge')).toHaveText('1')
    await expect(heading.locator('.unread-badge')).toHaveClass(/waiting/)
    await expect(heading).toHaveAttribute('aria-label', `${section}, Needs approval`)
    await heading.click()

    // The roster's Mark as Read leaves it counted too.
    await row.click({ button: 'right' })
    await page.getByRole('menuitem', { name: 'Mark as Read' }).click()
    await expect(row.locator('.unread-badge')).toHaveText('1')

    // Answered (the reply read in its open chat), it leaves the badge.
    await row.click()
    await page.locator('.permission-card .permission-option').last().click() // the deny answer
    await expect(page.locator('.permission-card')).toContainText('Denied')
    await page.locator('.roster-row', { hasText: other }).click()
    await expect(row.locator('.unread-badge')).toHaveCount(0)
  } finally {
    for (const botId of ids) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
