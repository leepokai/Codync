import { test, expect } from '@playwright/test'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach, setPref } from './sandbox.ts'

// A scripted agent that runs a foreground and a background subagent as Claude's adapter does
// (host/tests/fixtures/subagent_agent.py).
const subagentAgent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/subagent_agent.py')

test('a subagent’s steps sit under its launch, which says what it cost', async () => {
  const { browser, page } = await attach()
  let botId: string | undefined
  try {
    await setPref(page, 'showThinkingDetails', true)
    const name = `Subagents e2e ${Date.now()}`
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(subagentAgent), connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'go' })

    // While it works: the unfolded steps nest each subagent's calls under its launch.
    const pill = page.locator('.working').first()
    await pill.locator('.working-toggle').click()
    const live = pill.locator('.working-steps')
    await expect(live.locator('.trace-nested', { hasText: 'ls -la' })).toBeVisible()
    await expect(live.locator('.trace-nested', { hasText: 'pwd' })).toBeVisible()
    await expect(live.locator('.trace-nested', { hasText: 'Read notes.md' })).toHaveCount(0)
    // Handing its report back is the subagent's plumbing: not a step here, only in Full conversation.
    await expect(live).not.toContainText('SubagentHandback')
    await expect(live).toContainText('general-purpose · 42.3k tokens · 6 s')
    await expect(live).toContainText('Explore · in background')
    await page.screenshot({ path: test.info().outputPath('working-steps.png') })

    // Once it replied: Full conversation shows the same, in order.
    const answer = page.locator('.message.agent', { hasText: 'Both subagents are done.' }).last()
    await expect(answer).toBeVisible({ timeout: 20_000 })
    await answer.locator('.bubble').click({ button: 'right' })
    await page.getByRole('menuitem', { name: 'Show what it did' }).click()
    const trace = page.locator('.trace-content')
    await expect(trace).toContainText('general-purpose · 42.3k tokens · 6 s')
    const order = await trace.evaluate((el) => {
      const titles = ['Count files', 'ls -la', 'Print working directory', 'pwd', 'Read notes.md']
      const text = el.textContent ?? ''
      return titles.map((t) => [t, text.indexOf(t)] as const).sort((a, b) => a[1] - b[1]).map(([t]) => t)
    })
    expect(order).toEqual(['Count files', 'ls -la', 'Print working directory', 'pwd', 'Read notes.md'])
    await expect(trace.locator('.trace-nested')).toHaveCount(3)
    await page.screenshot({ path: test.info().outputPath('full-conversation.png') })
  } finally {
    await page.keyboard.press('Escape').catch(() => {})
    await setPref(page, 'showThinkingDetails', false).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
