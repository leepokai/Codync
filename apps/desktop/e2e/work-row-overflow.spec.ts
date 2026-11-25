import { test, expect, type Locator, type Page } from '@playwright/test'
import { api, attach, workBot } from './sandbox.ts'

// A thread whose task runs a long shell command (the scripted agent's `COMMAND`, host/tests/fixtures/work_agent.py):
// the command sits beside the row's title, and it must end with an ellipsis before the row's time, never run under it.
const COMMAND = 'agentctl wait'

/** Where a row's line, its text column and its time end or start, and whether the line is ellipsized. */
const measure = (row: Locator) =>
  row.evaluate((el) => {
    const open = el.querySelector('.work-row-open')!
    const column = open.querySelector(':scope > .vstack')!
    const line = column.querySelector('.work-row-line')!
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
    await cleanUp(page, botId)
    await browser.close()
  }
})

test('a long title gives way to how the row stands, which stays beside it, a gap apart', async () => {
  const { browser, page } = await attach()
  const name = `Row title e2e ${Date.now()}`
  const title = 'Read every file in the repository and write down what each one is for'
  let botId: string | undefined
  try {
    botId = (await workBot(page, name)).id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: `ADD ${title}|HOLD:title` })

    await page.locator('.work-capsule .work-capsule-open').click()
    const row = page.locator('.details-panel .work-row', { hasText: 'Read every file' })
    await expect(row.locator('.work-row-line')).not.toBeEmpty()
    const box = await row.evaluate((el) => {
      const rect = (q: string) => el.querySelector(q)!.getBoundingClientRect()
      const line = el.querySelector('.work-row-line')!
      const [name, status] = [rect('.work-row-name'), rect('.work-row-line')]
      return { gap: status.left - name.right, overlap: Math.min(name.bottom, status.bottom) - Math.max(name.top, status.top), width: status.width, truncated: line.scrollWidth > line.clientWidth }
    })
    expect(box.overlap).toBeGreaterThan(0)
    expect(box.gap).toBeGreaterThanOrEqual(7.5)
    expect(box.width).toBeGreaterThan(0)
    expect(box.truncated).toBe(false)
  } finally {
    await cleanUp(page, botId)
    await browser.close()
  }
})

async function cleanUp(page: Page, botId: string | undefined) {
  if (!botId) return
  await api(page, 'stopWork', { botId }).catch(() => {})
  await api(page, 'deleteBot', { botId }).catch(() => {})
}
