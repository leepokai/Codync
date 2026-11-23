import { test, expect, type Locator } from '@playwright/test'
import { writeFileSync } from 'node:fs'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, setPref, toolCallAgent } from './sandbox.ts'

// A scripted agent whose turn runs one long command and stays on it, so the chat's working pill shows it
// as its activity (a tool call's title).
const longActivity = 'cd "/Users/example/.codync/bots/0bcd2c28-ac38-4db8-b932-4ec2da5be7a3/workspace/uploads/628f8428-a7bd-48b1-8cfc-806332468f68"'
const longAgent = toolCallAgent([{ kind: 'execute', title: longActivity }])

/** Whether the pill's activity line ends inside the pill, is ellipsized, and leaves nothing painted outside. */
const fits = (pill: Locator) =>
  pill.evaluate((el) => {
    const activity = el.querySelector('.working-status .truncate')!
    const text = activity.getBoundingClientRect()
    const box = el.getBoundingClientRect()
    return {
      inside: text.right <= box.right + 0.5,
      truncated: activity.scrollWidth > activity.clientWidth,
      contained: el.scrollWidth <= el.clientWidth,
    }
  })

test('a long activity truncates inside the working pill', async () => {
  const { browser, page } = await attach()
  const script = test.info().outputPath('long_agent.py')
  writeFileSync(script, longAgent)
  let botId: string | undefined
  try {
    await setPref(page, 'showThinkingDetails', false)
    const name = `Long activity e2e ${Date.now()}`
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(script), connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'go' })

    const pill = page.locator('.working').first()
    await expect(pill).toContainText(longActivity)
    const inside = { inside: true, truncated: true, contained: true }
    expect(await fits(pill)).toEqual(inside)

    // With thinking details on, the status sits inside the toggle button: it must hold the same,
    // unfolded too, where the pill takes the whole column.
    await setPref(page, 'showThinkingDetails', true)
    await expect(pill.locator('.working-toggle')).toHaveCount(1)
    expect(await fits(pill)).toEqual(inside)
    await pill.locator('.working-toggle').click()
    await expect(pill).toHaveClass(/expanded/)
    expect(await fits(pill)).toEqual(inside)
    await page.screenshot({ path: test.info().outputPath('working-activity.png') })
  } finally {
    await setPref(page, 'showThinkingDetails', false).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
