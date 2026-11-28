import { test, expect } from '@playwright/test'
import { api, appPage, attach, openPage } from './sandbox.ts'

type View = { networks: { id: string; name: string; rules: unknown[] }[]; sessions?: { address: string; tier: string; registry?: string }[] }

// A coding agent's session registers on its first call (agentCall, as `codync-host mcp network` sends it).
const whoami = (session: string, kind: string) => ({
  agent: { kind, session, process: '1-e2e', cwd: '/work/api' },
  name: 'whoami',
  arguments: {},
})

test("the Network page lists this computer's coding agents, and a rule can only allow or deny one", async () => {
  test.setTimeout(90_000)
  const { browser, page } = await attach()
  let made: string | undefined
  try {
    const codex = await api<{ address: string }>(page, 'agentCall', whoami(`e2e-${Date.now()}`, 'codex'))
    const view = await api<View>(page, 'networkView', {})
    const listed = view.sessions?.find((s) => s.address === codex.address)
    expect(listed).toMatchObject({ tier: 'pull', registry: 'codex-acp' })

    await openPage(page, 'Network')
    const dialog = appPage(page)
    await expect(dialog.getByText('Agents', { exact: true })).toBeVisible()
    const row = dialog.locator('.settings-row', { hasText: codex.address })
    await expect(row).toContainText('Checks its mail')
    if (process.env.SHOT) await dialog.screenshot({ path: process.env.SHOT })

    test.skip(view.networks.length > 0, 'The sandbox is already in a network: its rules card would not be the only one.')
    const name = `e2e-${Date.now()}`
    await api(page, 'networkCreate', { network: name, connector: 'private' })
    made = (await api<View>(page, 'networkView', {})).networks.find((n) => n.name === name)?.id
    await dialog.getByRole('button', { name: 'Add rule' }).click()
    const sheet = page.locator('.network-sheet')
    await expect(sheet.locator('.choice-row')).toHaveCount(4)
    await sheet.getByText('Any bot here', { exact: true }).click()
    await page.getByRole('menuitemradio', { name: codex.address, exact: true }).click()
    // It reads its own mail: no one there to hold it to Auto or Ask.
    await expect(sheet.locator('.choice-row')).toHaveCount(2)
    await page.keyboard.press('Escape')
    await expect(sheet).toHaveCount(0)
    // One sent anyway (an older rule, the API) is saved as Deny.
    const saved = await api<View>(page, 'setNetworkRules', {
      network: made,
      rules: [{ fromComputer: '*', fromAgent: '*', toAgent: codex.address, access: 'ask' }],
    })
    expect(saved.networks.find((n) => n.id === made)?.rules[0]).toMatchObject({ toAgent: codex.address, access: 'deny' })
  } finally {
    if (made) await api(page, 'networkLeave', { network: made }).catch(() => {})
    await openPage(page, 'Bots').catch(() => {})
    await browser.close()
  }
})
