import { test, expect, type CDPSession, type Page } from '@playwright/test'
import { api, attach, setPref } from './sandbox.ts'

// The details panel shows a bot as a group's sheet shows it: its face, name and what runs it above the panes. Before,
// that block was mounted only for a group, so a lead bot's crown — the button that opens the role explanation — was
// never drawn in the panel, and the "the larger badge in details opens the lead agent role explanation" the docs
// promise could not be reached. The roster's crown stays decorative (its row says "Lead agent").

async function setViewport(cdp: CDPSession, width: number, height: number) {
  await cdp.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
  await new Promise((done) => setTimeout(done, 250))
}

/** The bot `name`'s chat, with its details panel open (a new chat starts with it closed). */
async function open(page: Page, name: string) {
  const row = page.locator('[data-roster-row]').filter({ has: page.getByText(name, { exact: true }) })
  await expect(row).toBeVisible()
  await row.click()
  await expect(page.locator('.title-pill')).toContainText(name)
  const panel = page.locator('.side-panel .details-panel')
  if (!(await panel.isVisible().catch(() => false))) await page.locator('.title-pill').click()
  await expect(panel).toBeVisible()
  return panel
}

test('a lead bot opens the role explanation from its details panel, and a plain bot shows no crown', async () => {
  test.setTimeout(120_000)
  const { browser, page } = await attach()
  const cdp = await page.context().newCDPSession(page)
  await setPref(page, 'macAccountOnboardingCompleted', true)
  await setPref(page, 'withoutAccount', true)
  if (await page.locator('.welcome').count()) await page.reload()
  await expect(page.locator('.welcome')).toHaveCount(0, { timeout: 15_000 })
  const run = Date.now()
  const lead = `Panel Lead ${run}`
  const plain = `Panel Plain ${run}`
  const group = `Panel Group ${run}`
  const made: string[] = []
  try {
    made.push((await api<{ bot: { id: string } }>(page, 'createBot', { name: plain, backend: 'claude' })).bot.id)
    made.push((await api<{ bot: { id: string } }>(page, 'createBot', { name: lead, backend: 'claude' })).bot.id)
    await api(page, 'setLead', { botId: made[1] })
    await setViewport(cdp, 1100, 760)

    // The lead's own chat, its panel open beside it: the bot's header with the crown on its face.
    const panel = await open(page, lead)
    const identity = panel.locator('.details-identity')
    await expect(identity.locator('.details-name')).toHaveText(lead)
    await expect(identity.locator('.avatar-frame .lead-agent-badge')).toHaveCount(1)

    // The crown is the panel's own button, drawn whole inside the panel (not clipped by its top edge).
    const crown = panel.getByRole('button', { name: 'About the lead agent' })
    await expect(crown).toBeVisible()
    const box = (await crown.boundingBox())!
    const frame = (await panel.boundingBox())!
    expect(box.y).toBeGreaterThanOrEqual(frame.y - 1)
    expect(box.x).toBeGreaterThanOrEqual(frame.x - 1)
    expect(box.x + box.width).toBeLessThanOrEqual(frame.x + frame.width + 1)

    // It opens the role explanation, and Done closes it.
    await crown.click()
    const explanation = page.getByRole('alertdialog')
    await expect(explanation).toBeVisible()
    await expect(explanation.locator('.dialog-card h2')).toHaveText('Lead agent')
    await expect(explanation.locator('.dialog-card')).toContainText('first point of contact')
    await explanation.getByRole('button', { name: 'Done' }).click()
    await expect(page.getByRole('alertdialog')).toHaveCount(0)

    // A keyboard reaches it too: Enter on the crown opens it, Escape closes it.
    await crown.focus()
    await page.keyboard.press('Enter')
    await expect(page.getByRole('alertdialog')).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(page.getByRole('alertdialog')).toHaveCount(0)

    // A bot that does not lead has the same header, and no crown to open.
    const plainPanel = await open(page, plain)
    await expect(plainPanel.locator('.details-identity .details-name')).toHaveText(plain)
    await expect(plainPanel.locator('.lead-agent-badge')).toHaveCount(0)

    // A group keeps its identity above its members, with no crown either.
    made.push((await api<{ bot: { id: string } }>(page, 'createBot', { name: group, kind: 'group', members: [...made] })).bot.id)
    const groupPanel = await open(page, group)
    await expect(groupPanel.locator('.details-identity .details-name')).toHaveText(group)
    await expect(groupPanel.locator('.details-members .member-row')).toHaveCount(3)
    await expect(groupPanel.locator('.lead-agent-badge')).toHaveCount(0)
  } finally {
    await cdp.send('Emulation.clearDeviceMetricsOverride').catch(() => {})
    for (const id of made.reverse()) await api(page, 'deleteBot', { botId: id }).catch(() => {})
    await browser.close()
  }
})
