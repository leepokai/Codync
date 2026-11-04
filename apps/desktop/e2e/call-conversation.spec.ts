import { test, expect, type Page } from '@playwright/test'
import { randomUUID } from 'node:crypto'
import { tmpdir } from 'node:os'
import { api, attach } from './sandbox.ts'

/** A call the host has ended, with what was said on it, as its line in the chat. */
async function endedCall(page: Page, botId: string, seconds: number, said: ['user' | 'voice', string][]) {
  const callId = randomUUID()
  await api(page, 'startCall', { botId, callId, mode: 'device' })
  const at = Date.now()
  const lines = said.map(([from, text], i) => ({ from, text, at: at + i * 1000, id: randomUUID() }))
  await api(page, 'endCall', { botId, callId, seconds, lines })
}

/** The colors of the call bar's dots, by how many opaque pixels have each. */
const dotColors = (page: Page) =>
  page.locator('canvas.call-dots').evaluate((canvas: HTMLCanvasElement) => {
    const data = canvas.getContext('2d')!.getImageData(0, 0, canvas.width, canvas.height).data
    const colors: Record<string, number> = {}
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] > 100) colors[`${data[i]},${data[i + 1]},${data[i + 2]}`] = (colors[`${data[i]},${data[i + 1]},${data[i + 2]}`] ?? 0) + 1
    }
    return Object.keys(colors)
  })

test("a call's conversation shows who and when on top, and a black bot's call dots stay visible", async () => {
  const { browser, page } = await attach()
  const name = `Call check ${randomUUID().slice(0, 6)}`
  const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
    name, avatarColor: 'black', avatarShape: 'blob', backend: 'claude', cwd: tmpdir(), connectors: [], skills: [],
  })
  try {
    await endedCall(page, bot.id, 3, [])
    await endedCall(page, bot.id, 16, [
      ['user', 'Can you check the build?'],
      ['voice', 'Sure, it passed a minute ago.'],
      ['voice', 'Two tests were skipped.'],
      ['user', 'Thanks, that is all.'],
      ['voice', 'You are welcome.'],
    ])
    await endedCall(page, bot.id, 185, Array.from({ length: 20 }, (_, i): ['user' | 'voice', string][] => [
      ['user', `Question ${i + 1} about the release?`],
      ['voice', `Answer ${i + 1}: it goes out once the review is green.`],
    ]).flat())

    await page.getByText(name, { exact: true }).first().click()
    const lines = page.locator('.call-line-row')
    await expect(lines).toHaveCount(3)
    const sheet = page.locator('.bot-conversation')
    const scroller = sheet.locator('.bot-conversation-scroll')

    // A short call: the header, then what was said, read from the top.
    await lines.nth(1).click()
    const header = sheet.locator('.call-header')
    await expect(header).toContainText(name)
    await expect(header).toContainText(/Today .+ · 00:16/)
    await expect(header.locator('.call-header-avatar > *').first()).toHaveCSS('width', '52px')
    await expect(sheet.getByText('You are welcome.')).toBeVisible()
    expect(await scroller.evaluate((el) => el.scrollTop)).toBe(0)
    // The voice's words in the text color, like the bot's name.
    const ink = await header.getByText(name).evaluate((el) => getComputedStyle(el).color)
    await expect(sheet.getByText('Sure, it passed a minute ago.')).toHaveCSS('color', ink)
    // Close within an exchange, wide where you speak after the other side.
    await expect(sheet.locator('.call-gap', { hasText: 'Two tests were skipped.' })).toHaveCSS('padding-top', '8px')
    await expect(sheet.locator('.call-gap-wide', { hasText: 'Thanks, that is all.' })).toHaveCSS('padding-top', '24px')
    await page.keyboard.press('Escape')
    await expect(sheet).toHaveCount(0)

    // A long call opens at its end.
    await lines.nth(2).click()
    await expect(sheet.getByText('Answer 20: it goes out once the review is green.')).toBeVisible()
    await expect.poll(() => scroller.evaluate((el) => el.scrollHeight - el.clientHeight - el.scrollTop)).toBeLessThan(2)
    await page.keyboard.press('Escape')
    await expect(sheet).toHaveCount(0)

    // A call where nothing was said says so under its header.
    await lines.nth(0).click()
    await expect(sheet.locator('.call-header')).toContainText(/· 00:03/)
    await expect(sheet.getByText('Nothing was said on this call.')).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(sheet).toHaveCount(0)

    // The call bar's dots are the ink color, never the bot's black, so they show on the dark bar too.
    await page.getByRole('button', { name: 'Call', exact: true }).click()
    await expect(page.locator('canvas.call-dots')).toBeVisible()
    for (const [scheme, ink] of [['light', '0,0,0'], ['dark', '255,255,255']] as const) {
      await page.emulateMedia({ colorScheme: scheme })
      await expect.poll(() => dotColors(page)).toEqual([ink])
    }
    await page.getByRole('button', { name: 'End call' }).click()
    await expect(page.locator('canvas.call-dots')).toHaveCount(0)
  } finally {
    await page.emulateMedia({ colorScheme: null })
    await api(page, 'deleteBot', { botId: bot.id }).catch(() => {})
    await browser.close()
  }
})
