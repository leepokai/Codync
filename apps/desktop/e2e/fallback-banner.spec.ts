import { test, expect } from '@playwright/test'
import { writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { api, attach, sandbox } from './sandbox.ts'

// A scripted agent that asks to sign in while `<name>.signedout` is in its folder (host/tests/fixtures/relay_agent.py).
const relayAgent = resolve(__dirname, '../../../host/tests/fixtures/relay_agent.py')
const agent = (name: string) => `python3 -X utf8 -u '${relayAgent}' ${name}`

type Bot = { id: string; agentStep?: unknown; resting?: { reason: string }[] }

// #444: a bot whose own agent is signed out says so on its Agent card, not that it waits for a usage reset.
test('the Agent card says the own agent is signed out while a fallback answers', async () => {
  const { browser, page } = await attach()
  const name = `Fallback banner e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // Kimi Code: a harness Codync knows that this computer doesn't run, played by the scripted agent.
    const { bot } = await api<{ bot: Bot }>(page, 'createBot', {
      name, backend: 'kimi', command: agent('primary'), connectors: [], skills: [], notify: false,
      fallbacks: [{ backend: 'codex', command: agent('standin'), model: null, modelOptions: {} }],
    })
    botId = bot.id
    writeFileSync(join(sandbox().home, 'bots', bot.id, 'workspace', 'primary.signedout'), '')
    await api(page, 'send', { botId: bot.id, text: 'Hello', clientNonce: `n-${Date.now()}` })
    const current = async () => (await api<{ bot: Bot }>(page, 'updateBot', { id: bot.id })).bot
    await expect.poll(async () => (await current()).resting?.[0]?.reason, { timeout: 30_000 }).toBe('signedOut')
    await expect.poll(async () => Boolean((await current()).agentStep), { timeout: 30_000 }).toBe(true)

    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: `${name}, view details` }).click()
    await page.locator('.details-panel').getByRole('tab', { name: 'Settings' }).click()
    const banner = page.locator('.details-panel').getByText(/^Using Codex /)
    await expect(banner).toHaveText('Using Codex while Kimi Code is signed out')
    await expect(banner).not.toContainText('resets')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
