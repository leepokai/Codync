import { test, expect } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type Connector = { id: string; name: string }

test('a custom connector can’t take an installed connector’s name, and the second Add changes nothing', async () => {
  const { browser, page } = await attach()
  const name = `Custom connector e2e ${Date.now()}`
  const named = async () => (await api<{ items: Connector[] }>(page, 'connectors', {})).items.filter((c) => c.name === name)
  let botId: string | undefined
  try {
    await page.reload()
    botId = (await api<{ bot: { id: string } }>(page, 'createBot', { name, backend: 'custom', command: 'unused', connectors: [], skills: [], notify: false })).bot.id
    await page.getByRole('button', { name: /Extensions/ }).click()
    await page.getByRole('tab', { name: 'Connectors' }).click()
    const sheet = page.locator('.modal-content', { has: page.getByPlaceholder('npx -y @scope/server') })
    const add = async (command: string) => {
      await page.locator('.capsule-action', { hasText: 'New' }).click()
      await sheet.getByPlaceholder('Name').fill(name)
      await sheet.getByPlaceholder('npx -y @scope/server').fill(command)
      await sheet.getByRole('button', { name: 'Add', exact: true }).click()
    }

    await add('node first.js')
    await expect(sheet).toBeHidden()
    const [first] = await named()
    expect(first).toBeDefined()

    await add('node second.js')
    await expect(sheet).toContainText(`A connector named "${name}" is already installed.`)
    expect(await named()).toEqual([first])
    // A no-op update reads the bot back: only the first connector was turned on for it.
    const { bot } = await api<{ bot: { connectors: string[] } }>(page, 'updateBot', { id: botId })
    expect(bot.connectors).toEqual([first.id])
  } finally {
    await page.keyboard.press('Escape')
    for (const c of await named().catch(() => [])) await api(page, 'removeConnector', { id: c.id }).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await page.reload() // Leaves Extensions, which would cover what the next spec clicks.
    await browser.close()
  }
})
