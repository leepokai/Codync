import { test, expect, type Page } from '@playwright/test'
import { resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { canSteer } from '../src/renderer/views/chat/queued-messages.ts'
import { api, attach, echoBot, steerBot, typeMessage } from './sandbox.ts'

type History = { entries: { id: string; seq: number; createdAt: number; kind: string; data: { text?: string; status?: string; notBefore?: number } }[] }

/** Whether the turn answering `text` has started. */
async function started(page: Page, botId: string, text: string): Promise<boolean> {
  const { entries } = await api<History>(page, 'history', { botId })
  return entries.some((e) => e.kind === 'user' && e.data.text === text && e.data.status === 'sent')
}

test('a long queue scrolls, with its oldest message in reach', async () => {
  const { browser, page } = await attach()
  const name = `Long queue e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await steerBot(page, name, 120)
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'send', { botId, text: 'first' })
    await expect.poll(() => started(page, bot.id, 'first')).toBe(true)
    for (let i = 1; i <= 40; i++) await api(page, 'send', { botId, text: `Waiting ${i}` })
    const queued = page.locator('.queued-row')
    await expect(queued).toHaveCount(40)

    await expect(page.locator('.composer textarea')).toBeInViewport()
    const list = page.locator('.queued-list')
    const box = await list.boundingBox()
    expect(box!.y).toBeGreaterThan(0)
    expect(box!.height).toBeLessThan(260)
    const row = (text: string) => queued.filter({ hasText: new RegExp(`^${text}$`) })
    await expect(row('Waiting 1')).toBeInViewport()
    await expect(row('Waiting 40')).not.toBeInViewport()

    await list.hover()
    await page.mouse.wheel(0, 2000)
    await expect(row('Waiting 40')).toBeInViewport()
    await row('Waiting 40').getByRole('button', { name: 'Remove' }).click()
    await expect(queued).toHaveCount(39)
    await page.mouse.wheel(0, -2000)
    await expect(row('Waiting 1')).toBeInViewport()

    // One typed here scrolls into view; the ones the computer got from elsewhere didn't.
    await typeMessage(page, 'typed here')
    await expect(row('typed here')).toBeInViewport()
  } finally {
    if (botId) {
      await api(page, 'stop', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})

test('a message sent while the bot works waits above the composer, then goes out now or is taken back', async () => {
  const { browser, page } = await attach()
  const name = `Queue e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const { version } = await api<{ version: string }>(page, 'hello', {})
    test.skip(!canSteer(version), `the sandbox host (${version}) can't steer yet`)
    const bot = await steerBot(page, name, 30)
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    const transcript = page.locator('.transcript')
    const queued = page.locator('.queued-row')

    await api(page, 'send', { botId, text: 'first' })
    await expect.poll(() => started(page, bot.id, 'first')).toBe(true)
    await typeMessage(page, 'second')
    await expect(queued.filter({ hasText: 'second' })).toBeVisible()
    await expect(transcript).not.toContainText('second')
    await queued.filter({ hasText: 'second' }).getByRole('button', { name: 'Send now' }).click()
    // It joined the running turn: the chat shows it, and the reply took it in.
    await expect(queued).toHaveCount(0)
    await expect(transcript).toContainText('second')
    await expect(transcript).toContainText('Got: ')

    await api(page, 'send', { botId, text: 'third' })
    await expect.poll(() => started(page, bot.id, 'third')).toBe(true)
    await typeMessage(page, 'never sent')
    await queued.filter({ hasText: 'never sent' }).getByRole('button', { name: 'Remove' }).click()
    await expect(queued).toHaveCount(0)
    await expect(transcript).not.toContainText('never sent')
    const { entries } = await api<History>(page, 'history', { botId })
    expect(entries.find((e) => e.data.text === 'never sent')).toBeUndefined()
  } finally {
    if (botId) {
      await api(page, 'stop', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})

test('a message that joined a turn its agent then drops without a word goes out again as the next turn', async () => {
  const { browser, page } = await attach()
  const name = `Queue drop e2e ${Date.now()}`
  let botId: string | undefined
  try {
    // Like Codex on a refused card: it takes the steered message in, then ends the turn as cancelled, saying nothing.
    const agent = resolve(__dirname, '../../../host/tests/fixtures/steer_agent.py')
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: `${fixtureCommand(agent)} drops 30`, connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    const queued = page.locator('.queued-row')

    await api(page, 'send', { botId, text: 'first' })
    await expect.poll(() => started(page, bot.id, 'first')).toBe(true)
    await typeMessage(page, 'second')
    await queued.filter({ hasText: 'second' }).getByRole('button', { name: 'Send now' }).click()
    // Not lost: the next turn answers it.
    await expect(page.locator('.transcript .message.agent', { hasText: 'Got: ' })).toContainText('second')
    await expect(queued).toHaveCount(0)
    const { entries } = await api<History>(page, 'history', { botId })
    expect(entries.filter((e) => e.kind === 'user' && e.data.text === 'second').map((e) => e.data.status)).toEqual(['sent'])
  } finally {
    if (botId) {
      await api(page, 'stop', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})

test('schedule edits preserve lines and IME composition, reject stale clear-time, and cancel without returning', async () => {
  const { browser, page } = await attach()
  const name = `Schedule edit e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'scheduleSend', {
      botId, text: 'first\nsecond', notBefore: Date.now() + 120_000, clientNonce: 'schedule-edit-e2e',
    })
    const row = page.locator('.queued-row')
    await expect(row).toHaveCount(1)
    await expect(row).toContainText('first')
    await expect(row.locator('.schedule-text, .schedule-time, .queued-edit')).toHaveCount(0)

    // The expanded editor and the inline editor both keep a multiline message as multiline.
    await row.getByRole('button', { name: 'Edit schedule' }).click()
    const sheet = page.locator('.schedule-sheet')
    await sheet.locator('.schedule-text').fill('edited\nin the sheet')
    await sheet.getByRole('button', { name: 'Save', exact: true }).click()
    await expect.poll(async () => {
      const { entries } = await api<History>(page, 'history', { botId })
      return entries[0]?.data.text
    }).toBe('edited\nin the sheet')

    await row.getByRole('button', { name: 'Edit', exact: true }).click()
    const inline = row.locator('.queued-edit')
    await expect(inline).toHaveValue('edited\nin the sheet')
    await inline.fill('edited\ninline')
    await inline.dispatchEvent('compositionstart')
    await inline.dispatchEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 229, which: 229, isComposing: true, bubbles: true, cancelable: true })
    // Enter selected an IME candidate: it neither committed nor closed the field.
    await expect(inline).toBeVisible()
    await expect(inline).toHaveValue('edited\ninline')
    await inline.dispatchEvent('compositionend')
    await inline.press('Enter')
    await expect.poll(async () => {
      const { entries } = await api<History>(page, 'history', { botId })
      return entries[0]?.data.text
    }).toBe('edited\ninline')

    // A second window's save makes this sheet stale. Clear-time must not send its older words.
    await row.getByRole('button', { name: 'Edit schedule' }).click()
    await api(page, 'updateScheduled', { botId, entryId: (await api<History>(page, 'history', { botId })).entries[0]!.id, text: 'newer from another window' })
    await sheet.getByRole('button', { name: 'Clear the time' }).click()
    await expect.poll(async () => {
      const { entries } = await api<History>(page, 'history', { botId })
      return [entries[0]?.data.text, entries[0]?.data.status]
    }).toEqual(['newer from another window', 'scheduled'])
    await expect(row).toHaveCount(1)
    const refusal = page.getByRole('alertdialog')
    await expect(refusal).toContainText('changed underneath you')
    await refusal.getByRole('button', { name: 'OK', exact: true }).click()

    await row.getByRole('button', { name: 'Remove', exact: true }).click()
    await expect(row).toHaveCount(0)
    const { entries } = await api<History>(page, 'history', { botId })
    expect(entries).toHaveLength(0)
  } finally {
    if (botId) {
      await api(page, 'stop', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})

test('a delivered schedule moves behind the chat written after it was scheduled', async () => {
  const { browser, page } = await attach()
  const name = `Schedule order e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await echoBot(page, name)
    botId = bot.id
    await page.locator('.roster-row', { hasText: name }).click()
    await api(page, 'scheduleSend', {
      botId, text: 'scheduled first, delivered last', notBefore: Date.now() + 120_000, clientNonce: 'schedule-order-e2e',
    })
    await api(page, 'send', { botId, text: 'written second, delivered second' })
    await expect(page.locator('.transcript')).toContainText('written second, delivered second')
    const row = page.locator('.queued-row')
    await expect(row).toHaveCount(1)
    await row.getByRole('button', { name: 'Send now', exact: true }).click()
    await expect(row).toHaveCount(0)
    await expect(page.locator('.transcript .message.user').last()).toContainText('scheduled first, delivered last')

    const { entries } = await api<History>(page, 'history', { botId })
    const scheduled = entries.find((e) => e.data.text === 'scheduled first, delivered last')!
    const ordinary = entries.find((e) => e.data.text === 'written second, delivered second')!
    expect(scheduled.seq).toBeGreaterThan(ordinary.seq)
    expect(scheduled.createdAt).toBeGreaterThanOrEqual(ordinary.createdAt)
    expect(entries.filter((e) => e.kind === 'user').at(-1)?.id).toBe(scheduled.id)
  } finally {
    if (botId) {
      await api(page, 'stop', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})
