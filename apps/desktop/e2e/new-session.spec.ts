import { test, expect } from '@playwright/test'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { api, attach, echoBot } from './sandbox.ts'

test("New session in a bot's menu starts its agent on a fresh session once confirmed, not when cancelled; the chat stays; a group has none", async () => {
  const { browser, page } = await attach()
  const name = `New session e2e ${Date.now()}`
  const cwd = mkdtempSync(join(tmpdir(), 'codync-new-session-'))
  // The scripted agent logs each session it opens to calls.jsonl in the bot's folder.
  const opened = () => readFileSync(join(cwd, 'calls.jsonl'), 'utf8').split('\n').filter((l) => l.includes('"session/new"')).length
  let botId: string | undefined
  let groupId: string | undefined
  try {
    botId = (await echoBot(page, name, cwd)).id
    // A session's first answer repeats the bot's instructions, so the messages are words those never hold.
    await api(page, 'send', { botId, text: 'ns-one' })
    const row = page.locator('.roster-row', { hasText: name })
    await row.click()
    await expect(page.locator('.message.agent', { hasText: 'ns-one' })).toBeVisible({ timeout: 20_000 })
    expect(opened()).toBe(1)

    const dialog = page.getByRole('alertdialog').filter({ hasText: 'Start a new session?' })
    const ask = async (answer: string) => {
      await row.click({ button: 'right' })
      await page.getByRole('menuitem', { name: 'New session' }).click()
      await dialog.getByRole('button', { name: answer }).click()
      await expect(dialog).toHaveCount(0)
    }
    await ask('Cancel')
    await api(page, 'send', { botId, text: 'ns-two' })
    await expect(page.locator('.message.agent', { hasText: 'ns-two' })).toBeVisible({ timeout: 20_000 })
    expect(opened(), 'cancelled, the session goes on').toBe(1)

    await ask('New session')
    await api(page, 'send', { botId, text: 'ns-three' })
    await expect(page.locator('.message.agent', { hasText: 'ns-three' })).toBeVisible({ timeout: 20_000 })
    expect(opened(), 'the next turn runs in a session of its own').toBe(2)
    await expect(page.locator('.message.agent', { hasText: 'ns-one' })).toBeVisible()

    // A group has no agent session of its own.
    const group = `New session group ${Date.now()}`
    groupId = (await api<{ bot: { id: string } }>(page, 'createBot', { name: group, kind: 'group', members: [botId] })).bot.id
    await page.locator('.roster-row', { hasText: group }).click({ button: 'right' })
    await expect(page.getByRole('menuitem', { name: 'Copy conversation ID' })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: 'New session' })).toHaveCount(0)
    await page.keyboard.press('Escape')
  } finally {
    if (groupId) await api(page, 'deleteBot', { botId: groupId }).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    rmSync(cwd, { recursive: true, force: true })
    await browser.close()
  }
})
