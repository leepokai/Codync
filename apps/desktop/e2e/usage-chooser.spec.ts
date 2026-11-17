import { test, expect, type Page } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type Usage = { providers: { id: string }[]; services: { id: string; name: string; shown: boolean }[] }

async function openChooser(page: Page) {
  await page.reload() // A settings window left open would cover the account button.
  await page.locator('.sidebar-account').click()
  await page.locator('.account-panel').getByText('Usage', { exact: true }).click()
  await page.getByRole('button', { name: 'Choose and arrange' }).first().click()
}

test('Settings > Usage chooses which agents show, and the host keeps the choice', async () => {
  const { browser, page } = await attach()
  const usage = () => api<Usage>(page, 'usage', {})
  const { services } = await usage()
  const service = services.find((s) => s.id === 'cursor') ?? services[0]
  test.skip(!service, 'no agent on this computer can report its usage')
  const shownAtStart = service.shown
  try {
    await api(page, 'setUsageShown', { id: service.id, shown: false })
    await openChooser(page)
    const row = page.locator('.settings-usage-chooser').getByRole('switch', { name: service.name })
    await expect(row).toHaveAttribute('aria-checked', 'false')

    await row.click()
    await expect(row).toHaveAttribute('aria-checked', 'true')
    expect((await usage()).services.find((s) => s.id === service.id)?.shown).toBe(true)

    await row.click()
    await expect(row).toHaveAttribute('aria-checked', 'false')
    const after = await usage()
    expect(after.services.find((s) => s.id === service.id)?.shown).toBe(false)
    expect(after.providers.some((p) => p.id === service.id)).toBe(false)
  } finally {
    await api(page, 'setUsageShown', { id: service.id, shown: shownAtStart }).catch(() => {})
    await page.reload().catch(() => {}) // Closes Settings, which would cover the next spec's window.
    await browser.close()
  }
})

test('dragging an agent in Settings > Usage changes its order, and the host keeps it', async () => {
  const { browser, page } = await attach()
  const order = async () => (await api<Usage>(page, 'usage', {})).services.map((s) => s.id)
  const before = await order()
  test.skip(before.length < 2, 'needs two agents that can report their usage')
  try {
    await openChooser(page)
    const rows = page.locator('.settings-usage-choice')
    await expect(rows).toHaveCount(before.length)

    const grip = await rows.nth(0).locator('.settings-usage-grip').boundingBox()
    const target = await rows.nth(1).boundingBox()
    if (!grip || !target) throw new Error('the rows have no box')
    await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2)
    await page.mouse.down()
    await page.mouse.move(grip.x + grip.width / 2, target.y + target.height / 2, { steps: 8 })
    await page.mouse.up()

    const swapped = [before[1], before[0], ...before.slice(2)]
    await expect.poll(order).toEqual(swapped)
    await expect(rows.nth(0)).toHaveAttribute('data-usage-service', before[1])

    await rows.nth(0).locator('.settings-usage-grip').focus()
    await page.keyboard.press('ArrowDown')
    await expect.poll(order).toEqual(before)
  } finally {
    await api(page, 'setUsageOrder', { ids: before }).catch(() => {})
    await page.reload().catch(() => {}) // Closes Settings, which would cover the next spec's window.
    await browser.close()
  }
})

// Guards the row staying where it was moved and the order the moves end on. It can't tell the order they reach the
// host in (latest-only.test.ts does), and the first answer can land before the second press.
test('two quick moves in Settings > Usage end on the second, and the row stays there', async () => {
  const { browser, page } = await attach()
  const order = async () => (await api<Usage>(page, 'usage', {})).services.map((s) => s.id)
  const before = await order()
  test.skip(before.length < 3, 'needs three agents that can report their usage')
  try {
    await openChooser(page)
    const rows = page.locator('.settings-usage-choice')
    await expect(rows).toHaveCount(before.length)

    await rows.nth(0).locator('.settings-usage-grip').focus()
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('ArrowDown')

    await expect.poll(order).toEqual([before[1], before[2], before[0], ...before.slice(3)])
    await expect(rows.nth(0)).toHaveAttribute('data-usage-service', before[1])
    await expect(rows.nth(2)).toHaveAttribute('data-usage-service', before[0])
  } finally {
    await api(page, 'setUsageOrder', { ids: before }).catch(() => {})
    await page.reload().catch(() => {}) // Closes Settings, which would cover the next spec's window.
    await browser.close()
  }
})
