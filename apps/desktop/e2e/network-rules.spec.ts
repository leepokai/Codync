import { test, expect } from '@playwright/test'
import { api, attach, openGeneralSettings } from './sandbox.ts'

type Rule = { fromComputer: string; fromAgent: string; toAgent: string; access: string }
type View = { networks: { id: string; name: string; rules: Rule[]; defaultAccess?: string }[] }

// Each level with the line under it, the most trusted first (docs/features/codync-network.md, Ceilings).
const LEVELS: [string, string][] = [
  ['Allow', 'Its requests run as the bot is set.'],
  ['Auto', 'The bot does by itself only what its agent judges safe, and asks you about the rest.'],
  ['Ask', 'Every change and command the bot makes for it waits for your approval.'],
  ['Deny', 'Its messages are refused.'],
]

test('a network rule takes one of four levels, and an Ask rule reads as one', async () => {
  const { browser, page } = await attach()
  let made: string | undefined
  try {
    const before = await api<View>(page, 'networkView', {})
    test.skip(before.networks.length > 0, 'The sandbox is already in a network: the rules card would not be the only one.')
    // Direct only: the network stays on this computer, with no relay to reach.
    const name = `e2e-${Date.now()}`
    await api(page, 'networkCreate', { network: name, connector: 'private' })
    const created = (await api<View>(page, 'networkView', {})).networks.find((n) => n.name === name)
    made = created?.id
    expect(made).toBeTruthy()
    // A computer no rule matches gets Auto, and the footer under the rules says so.
    expect(created?.defaultAccess).toBe('auto')

    await openGeneralSettings(page)
    const dialog = page.getByRole('dialog')
    await dialog.getByRole('button', { name: 'Network', exact: true }).click()
    await expect(dialog.getByText('A computer no rule matches gets Auto.', { exact: false })).toBeVisible()
    await dialog.getByRole('button', { name: 'Add rule' }).click()
    const sheet = page.locator('.network-sheet')
    const rows = sheet.locator('.choice-row')
    await expect(rows).toHaveCount(LEVELS.length)
    for (const [i, [label, detail]] of LEVELS.entries()) await expect(rows.nth(i)).toHaveText(label + detail)
    // A new rule starts at Deny.
    await expect(rows.nth(3).locator('.check-slot')).toHaveClass(/\bon\b/)

    await rows.nth(2).click()
    await expect(rows.nth(2).locator('.check-slot')).toHaveClass(/\bon\b/)
    await sheet.getByRole('button', { name: 'Add rule' }).click()
    await expect(sheet).toHaveCount(0)
    await expect(dialog.getByText('Ask: any bot on any computer → any bot here', { exact: true })).toBeVisible()
    const saved = (await api<View>(page, 'networkView', {})).networks.find((n) => n.id === made)?.rules
    expect(saved).toEqual([{ fromComputer: '*', fromAgent: '*', toAgent: '*', access: 'ask' }])
  } finally {
    if (made) await api(page, 'networkLeave', { network: made }).catch(() => {})
    await page.keyboard.press('Escape')
    await browser.close()
  }
})
