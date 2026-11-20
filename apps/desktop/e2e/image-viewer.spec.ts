import { test, expect, type Page } from '@playwright/test'
import { execFileSync } from 'node:child_process'
import { api, attach, echoBot, steerBot } from './sandbox.ts'

/** A `width` x `height` PNG drawn in the app, as base64. */
function pngBase64(page: Page, width: number, height: number) {
  return page.evaluate(([width, height]) => {
    const canvas = document.createElement('canvas')
    canvas.width = width
    canvas.height = height
    const context = canvas.getContext('2d')!
    context.fillStyle = '#3a7'
    context.fillRect(0, 0, width, height)
    return canvas.toDataURL('image/png').split(',')[1]
  }, [width, height] as const)
}

/** Sends the bot a `width` x `height` picture, as if from the composer. */
async function sendPicture(page: Page, botId: string, width = 400, height = 300) {
  const uploadId = crypto.randomUUID()
  await api(page, 'upload', { botId, uploadId, name: 'chart.png', data: await pngBase64(page, width, height), offset: 0, done: true })
  await api(page, 'send', { botId, text: '', clientNonce: crypto.randomUUID(), attachments: [uploadId] })
}

test('a sent picture opens full size, zooms, copies, and offers Copy image and Copy name on right-click', async () => {
  const { browser, page } = await attach()
  const name = `Image viewer e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await steerBot(page, name, 60)
    botId = bot.id
    await sendPicture(page, botId)
    await page.locator('.roster-row', { hasText: name }).click()

    await page.locator('.attachment-image-button').last().click()
    const box = page.locator('.attachment-viewer .zoom-box')
    await expect(box).toBeVisible()
    await expect(box).not.toHaveClass(/zoomed/)
    await box.dblclick()
    await expect(box).toHaveClass(/zoomed/)
    await page.keyboard.press('0')
    await expect(box).not.toHaveClass(/zoomed/)
    await page.keyboard.press('+')
    await expect(box).toHaveClass(/zoomed/)

    await page.getByRole('button', { name: 'Copy image' }).click()
    await expect(page.getByRole('button', { name: 'Copied' })).toBeVisible()

    await page.keyboard.press('Escape')
    await expect(box).toBeHidden()
    await page.locator('.attachment-image-button').last().click({ button: 'right' })
    await expect(page.getByRole('menuitem', { name: 'Copy image' })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: 'Copy name' })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: 'Copy', exact: true })).toHaveCount(0)
    await page.keyboard.press('Escape')
  } finally {
    if (botId) {
      await api(page, 'stop', { botId }).catch(() => {})
      await api(page, 'deleteBot', { botId }).catch(() => {})
    }
    await browser.close()
  }
})

test('closing the picture viewer throws nothing', async () => {
  const { browser, page } = await attach()
  const name = `Viewer close e2e ${Date.now()}`
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(`${error.name}: ${error.message}`))
  let botId: string | undefined
  try {
    botId = (await echoBot(page, name)).id
    await sendPicture(page, botId)
    await page.locator('.roster-row', { hasText: name }).click()
    const box = page.locator('.attachment-viewer .zoom-box')
    // The picture's box leaves the page after the sheet's exit animation; its resize observer used to run once more.
    for (let i = 0; i < 15; i++) {
      await page.locator('.attachment-image-button').last().click()
      await expect(box).toBeVisible()
      await page.keyboard.press('Escape')
      await expect(box).toHaveCount(0)
    }
    expect(errors).toEqual([])
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})

test("a message's Copy right after Copy image keeps the message's text on the clipboard", async () => {
  test.skip(process.platform !== 'darwin', 'reads the clipboard with pbpaste')
  const { browser, page } = await attach()
  const name = `Copy order e2e ${Date.now()}`
  const text = `copy me ${Date.now()}`
  let botId: string | undefined
  try {
    botId = (await echoBot(page, name)).id
    await sendPicture(page, botId, 1600, 1200)
    await api(page, 'send', { botId, text, clientNonce: crypto.randomUUID() })
    await page.locator('.roster-row', { hasText: name }).click()
    const bubble = page.locator('.bubble.user', { hasText: text })
    await expect(bubble).toBeVisible()

    await page.locator('.attachment-image-button').last().click({ button: 'right' })
    await page.getByRole('menuitem', { name: 'Copy image' }).click()
    await bubble.click({ button: 'right' })
    await page.getByRole('menuitem', { name: 'Copy', exact: true }).click()
    await expect.poll(() => execFileSync('pbpaste').toString()).toBe(text)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
