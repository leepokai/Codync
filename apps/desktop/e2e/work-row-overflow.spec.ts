import { test, expect, type Locator } from '@playwright/test'
import { api, attach, workBot } from './sandbox.ts'

// A thread whose task runs a long shell command (the scripted agent's `COMMAND`, host/tests/fixtures/work_agent.py):
// the command is the row's second line, and it must end with an ellipsis before the row's time, never run under it.
const COMMAND = 'agentctl wait'

/** Where a row's line, its text column and its time end or start, and whether the line is ellipsized. */
const measure = (row: Locator) =>
  row.evaluate((el) => {
    const open = el.querySelector('.work-row-open')!
    const column = open.querySelector(':scope > .vstack')!
    const line = column.querySelector(':scope > .truncate')!
    const time = open.lastElementChild!
    return {
      lineRight: line.getBoundingClientRect().right,
      columnRight: column.getBoundingClientRect().right,
      timeLeft: time.getBoundingClientRect().left,
      truncated: line.scrollWidth > line.clientWidth,
    }
  })

test("a thread's long command ends with an ellipsis before the row's time", async () => {
  const { browser, page } = await attach()
  const name = `Row overflow e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await workBot(page, name)).id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'ADD Watch the agents|COMMAND HOLD:overflow' })

    await page.locator('.work-capsule .work-capsule-open').click()
    const details = page.locator('.details-panel')
    const row = details.locator('.work-row[data-work-status="working"]', { hasText: 'Watch the agents' })
    await expect(row).toContainText(COMMAND)
    await expect(row).toContainText(/\d+:\d\d/)

    const box = await measure(row)
    expect(box.truncated).toBe(true)
    expect(box.lineRight).toBeLessThanOrEqual(box.columnRight + 0.5)
    expect(box.lineRight).toBeLessThanOrEqual(box.timeLeft)
    await page.screenshot({ path: test.info().outputPath('work-row-overflow.png') })
  } finally {
    if (botId) {
      await api(page, 'stopWork', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})
