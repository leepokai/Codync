import { test, expect } from '@playwright/test'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, setPref, typeMessage } from './sandbox.ts'

// A scripted agent that, for CHAIN, hands the chain on with message_bot to the bot named in handoff.json in its folder
// (host/tests/fixtures/team_agent.py). Two of them pass it back and forth until the handoff limit stops it.
const agent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/team_agent.py')

test('a chain stopped at the handoff limit leaves a notice whose Continue sends the bot "Continue", once', async () => {
  const { browser, page } = await attach()
  const stamp = Date.now()
  const folder = mkdtempSync(join(tmpdir(), 'chain-continue-'))
  const bots: string[] = []
  try {
    await setPref(page, 'appLanguage', 'en')
    await page.reload()
    const make = async (name: string) => {
      const cwd = join(folder, name)
      mkdirSync(cwd)
      const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
        name: `${name} ${stamp}`, backend: 'custom', command: fixtureCommand(agent), cwd, connectors: [], skills: [], notify: false,
      })
      bots.push(bot.id)
      return { id: bot.id, cwd }
    }
    const first = await make('Chain first')
    const second = await make('Chain second')
    writeFileSync(join(first.cwd, 'handoff.json'), JSON.stringify({ target: second.id }))
    writeFileSync(join(second.cwd, 'handoff.json'), JSON.stringify({ target: first.id }))

    await page.locator('.roster-row', { hasText: `Chain first ${stamp}` }).click()
    await typeMessage(page, 'CHAIN')
    const transcript = page.locator('.transcript')
    const notice = transcript.getByText('This work went on 8 steps in a row without you, the most it can. Continue to let it go on.')
    await expect(notice).toBeVisible({ timeout: 30_000 })
    const button = transcript.getByRole('button', { name: 'Continue', exact: true })
    await expect(button).toHaveCount(1)
    await button.click()
    // The user's own "Continue" goes to the bot, and the button is gone for good.
    await expect(transcript.locator('.bubble.user', { hasText: /^Continue$/ })).toHaveCount(1)
    await expect(transcript.getByRole('button', { name: 'Continue', exact: true })).toHaveCount(0)
    await expect(notice).toBeVisible()
    // A second press, from a client that still showed the button, is refused and sends nothing.
    const { entries } = await api<{ entries: { id: string; data: { continueChain?: boolean } }[] }>(page, 'history', { botId: first.id, limit: 100 })
    const pressed = entries.find((e) => e.data.continueChain === false)
    expect(pressed).toBeDefined()
    await expect(api(page, 'continueChain', { entryId: pressed?.id })).rejects.toThrow()
    await expect(transcript.locator('.bubble.user', { hasText: /^Continue$/ })).toHaveCount(1)
  } finally {
    for (const botId of bots) {
      await api(page, 'stop', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    rmSync(folder, { recursive: true, force: true })
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await page.reload()
    await browser.close()
  }
})
