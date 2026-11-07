import { test, expect, type Page } from '@playwright/test'
import { api, attach } from './sandbox.ts'

/** How many messages the user has sent to `botId`. */
async function sent(page: Page, botId: string) {
  const { entries } = await api<{ entries: { kind: string }[] }>(page, 'history', { botId })
  return entries.filter((e) => e.kind === 'user').length
}

/** Opens a new group of these members (`[name, description]`), runs `body` in it, and deletes it all again. */
async function inGroup(members: [string, string][], body: (page: Page, groupId: string) => Promise<void>) {
  const { browser, page } = await attach()
  const made: string[] = []
  const create = async (body: object) => {
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', body)
    made.push(bot.id)
    return bot.id
  }
  try {
    const ids: string[] = []
    for (const [name, description] of members) {
      ids.push(await create({ name, description, backend: 'custom', command: 'unused', connectors: [], permission: 'auto' }))
    }
    const name = `Mention Group ${Date.now()}`
    const groupId = await create({ name, kind: 'group', members: ids })
    await page.locator('.roster-row', { hasText: name }).click()
    await body(page, groupId)
    await page.locator('.composer textarea').fill('')
  } finally {
    for (const botId of made.reverse()) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
}

// Short enough that a whole name still counts as a mention being typed (24 characters).
const stamp = () => Date.now().toString(36)

test('typing @ in a group lists its members one per row, and the keyboard or the mouse picks one', async () => {
  const alpha = `Mention Alpha ${stamp()}`
  const beta = `Mention Beta ${stamp()}`
  await inGroup([[alpha, 'Reviews the code'], [beta, 'Writes the docs']], async (page, groupId) => {
    const field = page.locator('.composer textarea')
    const list = page.getByRole('listbox', { name: 'Members' })
    const rows = list.getByRole('option')

    // One row per member, under each other, with its description.
    await field.fill('@')
    await expect(rows).toHaveCount(2)
    await expect(rows.nth(0)).toContainText('Reviews the code')
    const [top, bottom] = [await rows.nth(0).boundingBox(), await rows.nth(1).boundingBox()]
    expect(bottom!.y).toBeGreaterThanOrEqual(top!.y + top!.height)
    expect(bottom!.x).toBe(top!.x)

    // ↑/↓ move the highlight, wrapping around the ends.
    await expect(rows.nth(0)).toHaveAttribute('aria-selected', 'true')
    await field.press('ArrowDown')
    await expect(rows.nth(1)).toHaveAttribute('aria-selected', 'true')
    await field.press('ArrowDown')
    await expect(rows.nth(0)).toHaveAttribute('aria-selected', 'true')
    await field.press('ArrowUp')
    await expect(rows.nth(1)).toHaveAttribute('aria-selected', 'true')

    // Return picks it and sends nothing; the next Return sends.
    await field.press('Enter')
    await expect(field).toHaveValue(`@${beta} `)
    await expect(list).toHaveCount(0)
    expect(await sent(page, groupId)).toBe(0)
    await field.press('Enter')
    await expect(field).toHaveValue('')
    await expect.poll(() => sent(page, groupId)).toBe(1)

    // Return picks even with the whole name typed.
    await field.fill(`@${alpha}`)
    await expect(rows).toHaveCount(1)
    await field.press('Enter')
    await expect(field).toHaveValue(`@${alpha} `)
    expect(await sent(page, groupId)).toBe(1)

    // Typing filters the list, after earlier words too, and Tab picks.
    await field.fill('hello @alp')
    await expect(rows).toHaveCount(1)
    await field.press('Tab')
    await expect(field).toHaveValue(`hello @${alpha} `)

    // Off the `@partial` (caret moved away, or text selected), the keys edit the draft and Return sends.
    await field.fill('hello @')
    await expect(rows).toHaveCount(2)
    await field.press('Home')
    await field.press('ArrowDown')
    await expect(rows.nth(0)).toHaveAttribute('aria-selected', 'true')
    await field.press('Home')
    await field.press('Enter')
    await expect(field).toHaveValue('')
    await expect.poll(() => sent(page, groupId)).toBe(2)
    await field.fill('hello @alp')
    await expect(rows).toHaveCount(1)
    await field.press('Shift+ArrowLeft')
    await field.press('Enter')
    await expect(field).toHaveValue('')
    await expect.poll(() => sent(page, groupId)).toBe(3)

    // Escape there still hides the list first, and keeps the reply being written.
    const message = page.locator('.message.user').last()
    await message.hover()
    await message.getByRole('button', { name: 'Reply' }).click()
    const quote = page.locator('.composer-reply')
    await expect(quote).toBeVisible()
    await field.fill('see @')
    await expect(rows).toHaveCount(2)
    await field.press('Home')
    await field.press('Escape')
    await expect(list).toHaveCount(0)
    await expect(quote).toBeVisible()
    await field.press('Escape')
    await expect(quote).toHaveCount(0)

    // An input method's Return confirms its candidate, not a member.
    await field.fill('@')
    await expect(rows).toHaveCount(2)
    await field.evaluate((el) => el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', keyCode: 229, bubbles: true, cancelable: true })))
    await expect(field).toHaveValue('@')
    await expect(rows).toHaveCount(2)

    // Escape hides the list and leaves the draft, until the draft changes.
    await field.press('Escape')
    await expect(list).toHaveCount(0)
    await expect(field).toHaveValue('@')
    await field.fill('')
    await field.fill('@')
    await expect(rows).toHaveCount(2)

    // The mouse works too: hovering moves the highlight, and the caret stays in the field.
    await rows.nth(1).hover()
    await expect(rows.nth(1)).toHaveAttribute('aria-selected', 'true')
    await rows.nth(1).click()
    await expect(field).toHaveValue(`@${beta} `)
    await expect(field).toBeFocused()
  })
})

test('Return sends once the mention is whole, even with a longer name or @all in reach', async () => {
  const nest = `Nest ${stamp()}`
  const allen = `Allen ${stamp()}`
  await inGroup([[nest, ''], [`${nest} Two`, ''], [allen, '']], async (page, groupId) => {
    const field = page.locator('.composer textarea')
    const list = page.getByRole('listbox', { name: 'Members' })
    const rows = list.getByRole('option')

    // A picked name that a longer one holds: the list is gone, so the next Return sends.
    await field.fill(`@${nest}`)
    await expect(rows).toHaveCount(2)
    await field.press('Enter')
    await expect(field).toHaveValue(`@${nest} `)
    await expect(list).toHaveCount(0)
    await field.press('Enter')
    await expect(field).toHaveValue('')
    await expect.poll(() => sent(page, groupId)).toBe(1)

    // Typing on can still reach the longer name.
    await field.fill(`@${nest} T`)
    await expect(rows).toHaveCount(1)

    // @all sends to the room although a member's name holds "all"; Tab can still pick that member.
    await field.fill('thanks @all')
    await expect(rows).toHaveCount(1)
    await field.press('Tab')
    await expect(field).toHaveValue(`thanks @${allen} `)
    await field.fill('thanks @all')
    await expect(rows).toHaveCount(1)
    await field.press('Enter')
    await expect(field).toHaveValue('')
    await expect.poll(() => sent(page, groupId)).toBe(2)

    // An @ and a space is ordinary text.
    await field.fill('meet @ two')
    await expect(list).toHaveCount(0)
  })
})
