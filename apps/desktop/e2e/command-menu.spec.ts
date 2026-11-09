import { test, expect } from '@playwright/test'
import { api, attach } from './sandbox.ts'

test('typing / lists the bot\'s commands, and the keyboard moves, picks and hides', async () => {
  const { browser, page } = await attach()
  const run = Date.now().toString(36)
  const name = `Command Bot ${run}`
  const { skill } = await api<{ skill: { id: string } }>(page, 'installSkill', { name: `Menu Skill ${run}`, description: 'Checks the list', instructions: 'Say hi.' })
  let botId: string | undefined
  const field = page.locator('.composer textarea')
  const list = page.getByRole('listbox', { name: 'Commands' })
  const rows = list.getByRole('option')
  const sent = async () => {
    const { entries } = await api<{ entries: { kind: string }[] }>(page, 'history', { botId })
    return entries.filter((e) => e.kind === 'user').length
  }
  try {
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', { name, backend: 'custom', command: 'unused', connectors: [], skills: [skill.id], permission: 'auto' })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()

    // Fast mode first, then the bot's skill.
    await field.fill('/')
    await expect(rows).toHaveCount(2)
    await expect(rows.nth(0)).toContainText('Fast mode')
    await expect(rows.nth(1)).toContainText('Checks the list')
    // The outline every floating layer has.
    expect(await page.locator('.composer-menu').evaluate((el) => getComputedStyle(el, '::after').borderTopWidth)).toBe('1px')

    // ↑/↓ move the highlight, wrapping around the ends.
    await expect(rows.nth(0)).toHaveAttribute('aria-selected', 'true')
    await field.press('ArrowDown')
    await expect(rows.nth(1)).toHaveAttribute('aria-selected', 'true')
    await field.press('ArrowDown')
    await expect(rows.nth(0)).toHaveAttribute('aria-selected', 'true')
    await field.press('ArrowUp')
    await expect(rows.nth(1)).toHaveAttribute('aria-selected', 'true')

    // Off the end of the draft, they move the caret and leave the highlight.
    await field.press('Home')
    await field.press('ArrowDown')
    await expect(rows.nth(1)).toHaveAttribute('aria-selected', 'true')
    await field.press('End')

    // Return fills in the skill and sends nothing.
    await field.press('Enter')
    await expect(field).toHaveValue(`/${skill.id} `)
    await expect(list).toHaveCount(0)
    expect(await sent()).toBe(0)

    // Tab picks a partly typed one.
    await field.fill(`/${skill.id.slice(0, -1)}`)
    await expect(rows).toHaveCount(1)
    await field.press('Tab')
    await expect(field).toHaveValue(`/${skill.id} `)

    // Escape hides the list and leaves the draft, until the draft changes.
    await field.fill('/')
    await expect(rows).toHaveCount(2)
    await field.press('Escape')
    await expect(list).toHaveCount(0)
    await expect(field).toHaveValue('/')
    await field.fill('')
    await field.fill('/')
    await expect(rows).toHaveCount(2)

    // Return runs Fast mode here: the box empties and the row now offers the opposite.
    await field.fill('/fa')
    await expect(rows).toHaveCount(1)
    const before = await rows.nth(0).textContent()
    await field.press('Enter')
    await expect(field).toHaveValue('')
    expect(await sent()).toBe(0)
    await field.fill('/fa')
    await expect(rows.nth(0)).not.toHaveText(before!)

    // Return on a skill whose name is typed out sends the message.
    await field.fill(`/${skill.id}`)
    await expect(rows).toHaveCount(1)
    await field.press('Enter')
    await expect(field).toHaveValue('')
    await expect.poll(sent).toBe(1)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await api(page, 'removeSkill', { id: skill.id }).catch(() => {})
    await browser.close()
  }
})
