import { test, expect, type Page } from '@playwright/test'
import { writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { api, attach, sandbox } from './sandbox.ts'

// A scripted agent that asks to sign in while `<name>.signedout` is in its folder (host/tests/fixtures/relay_agent.py).
const relayAgent = resolve(__dirname, '../../../host/tests/fixtures/relay_agent.py')
const agent = (name: string) => `python3 -X utf8 -u '${relayAgent}' ${name}`

type Bot = { id: string; agentStep?: unknown; resting?: { reason: string }[] }

/** A Kimi Code bot (a harness Codync knows that this computer doesn't run, played by the scripted agent) that is signed out, so its Codex fallback answers. */
async function signedOutBot(page: Page, name: string): Promise<string> {
  const { bot } = await api<{ bot: Bot }>(page, 'createBot', {
    name, backend: 'kimi', command: agent('primary'), connectors: [], skills: [], notify: false,
    fallbacks: [{ backend: 'codex', command: agent('standin'), model: null, modelOptions: {} }],
  })
  try {
    writeFileSync(join(sandbox().home, 'bots', bot.id, 'workspace', 'primary.signedout'), '')
    await api(page, 'send', { botId: bot.id, text: 'Hello', clientNonce: `n-${Date.now()}` })
    const current = async () => (await api<{ bot: Bot }>(page, 'updateBot', { id: bot.id })).bot
    await expect.poll(async () => (await current()).resting?.[0]?.reason, { timeout: 30_000 }).toBe('signedOut')
    await expect.poll(async () => Boolean((await current()).agentStep), { timeout: 30_000 }).toBe(true)
    return bot.id
  } catch (error) {
    // The caller only learns the id on success, so a failed setup deletes its own bot.
    await api(page, 'deleteBot', { botId: bot.id }).catch(() => {})
    throw error
  }
}

// #444: a bot whose own agent is signed out says so on its Agent card, not that it waits for a usage reset.
test('the Agent card says the own agent is signed out while a fallback answers', async () => {
  const { browser, page } = await attach()
  const name = `Fallback banner e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await signedOutBot(page, name)
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

// #461: the banner and the own agent's rest line read in the app's language, with the names put in after.
test('the fallback banner and rest line follow the app language', async () => {
  const { browser, page } = await attach()
  const name = `Fallback banner zh e2e ${Date.now()}`
  let botId: string | undefined
  try {
    botId = await signedOutBot(page, name)
    await page.evaluate(() => localStorage.setItem('appLanguage', JSON.stringify('zh-Hant')))
    await page.reload()
    await page.locator('.roster-row', { hasText: name }).click()
    await page.getByRole('button', { name: `${name}，檢視詳情` }).click()
    const panel = page.locator('.details-panel')
    await panel.getByRole('tab', { name: '設定' }).click()
    await expect(panel.locator('.fallback-switch-back').locator('..')).toContainText('正在使用 Codex，Kimi Code 已登出')
    await panel.locator('.agent-picker-summary').first().click()
    await expect(panel.getByText(/^休息到 /)).toBeVisible()
    await expect(panel.getByText(/^Resting until /)).toHaveCount(0)
  } finally {
    await page.evaluate(() => localStorage.removeItem('appLanguage')).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await page.reload().catch(() => {})
    await browser.close()
  }
})
