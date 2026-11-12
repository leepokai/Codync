import { test, expect, type Page } from '@playwright/test'
import { api, attach, steerBot } from './sandbox.ts'

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

test('a sent picture opens full size, zooms, copies, and offers Copy image and Copy name on right-click', async () => {
  const { browser, page } = await attach()
  const name = `Image viewer e2e ${Date.now()}`
  let botId: string | undefined
  try {
    const bot = await steerBot(page, name, 60)
    botId = bot.id
    const uploadId = crypto.randomUUID()
    await api(page, 'upload', { botId, uploadId, name: 'chart.png', data: await pngBase64(page, 400, 300), offset: 0, done: true })
    await api(page, 'send', { botId, text: '', clientNonce: crypto.randomUUID(), attachments: [uploadId] })
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
