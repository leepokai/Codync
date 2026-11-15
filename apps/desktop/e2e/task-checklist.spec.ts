import { test, expect } from '@playwright/test'
import { api, attach, echoBot, typeMessage } from './sandbox.ts'

type Step = { content: string; status: 'pending' | 'in_progress' | 'completed' }

test("a bot's update_plan shows its checklist above the box, which folds and leaves when the work is done", async () => {
  const { browser, page } = await attach()
  const name = `Checklist Bot ${Date.now().toString(36)}`
  let botId: string | undefined
  const card = page.locator('.task-checklist')
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    const plan = (items: Step[]) => api<{ saved: boolean; steps: number }>(page, 'chatCall', { botId, name: 'update_plan', arguments: { items } })
    await page.locator('.roster-row', { hasText: name }).click()

    // The checklist is the latest turn's, so the chat has one.
    await typeMessage(page, 'Plan the release')
    await expect(page.locator('.chat-row', { hasText: 'Plan the release' }).first()).toBeVisible()
    await expect(card).toHaveCount(0)

    const steps: Step[] = [
      { content: 'Read the changelog', status: 'completed' },
      { content: 'Write the release notes', status: 'in_progress' },
      { content: 'Tag the release', status: 'pending' },
    ]
    expect(await plan(steps)).toEqual({ saved: true, steps: 3 })
    await expect(card.locator('.task-checklist-title')).toBeVisible()
    await expect(card.locator('.task-checklist-count')).toHaveText('1/3')
    await expect(card.getByRole('listitem')).toHaveCount(3)
    await expect(card.locator('.task-checklist-done')).toHaveText('Read the changelog')

    // Another call replaces the list in place: one checklist, now further along.
    await plan(steps.map((s, i) => (i === 1 ? { ...s, status: 'completed' } : i === 2 ? { ...s, status: 'in_progress' } : s)))
    await expect(card.locator('.task-checklist-count')).toHaveText('2/3')
    await expect(card).toHaveCount(1)
    const { entries } = await api<{ entries: { kind: string }[] }>(page, 'history', { botId })
    expect(entries.filter((e) => e.kind === 'plan')).toHaveLength(1)

    // A click folds it to the step under way, and again opens it.
    await card.getByRole('button').click()
    await expect(card.getByRole('listitem')).toHaveCount(0)
    await expect(card).toContainText('Tag the release')
    await card.getByRole('button').click()
    await expect(card.getByRole('listitem')).toHaveCount(3)

    // The host refuses what isn't a checklist.
    await expect(api(page, 'chatCall', { botId, name: 'update_plan', arguments: { items: [{ content: 'x', status: 'done' }] } })).rejects.toThrow()

    // Every step done and the bot idle: it leaves. The full conversation keeps it.
    await plan(steps.map((s) => ({ ...s, status: 'completed' as const })))
    await expect(card).toHaveCount(0)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
