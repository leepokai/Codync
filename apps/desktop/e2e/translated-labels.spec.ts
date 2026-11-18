import { test, expect } from '@playwright/test'
import { api, attach, echoBot, typeMessage } from './sandbox.ts'

// Tooltips and screen-reader names that carry a name, an emoji or a count, and the chat's round buttons,
// read in the app language (#462).
test('tooltips and accessible names read in the app language', async () => {
  const { browser, page } = await attach()
  let botId: string | undefined
  try {
    await page.evaluate(() => localStorage.setItem('appLanguage', JSON.stringify('zh')))
    await page.reload()
    const name = `Translated labels e2e ${Date.now()}`
    botId = (await echoBot(page, name)).id
    await page.locator('.roster-row', { hasText: name }).click()

    await expect(page.locator('.computer-filter').first()).toHaveAttribute('title', /台已连接$/)
    await expect(page.locator('.top-chrome-row .mac-chat-button')).toHaveAttribute('title', '对话详情')

    await typeMessage(page, 'hello')
    await page.locator('.bubble.user', { hasText: 'hello' }).click({ button: 'right' })
    await page.locator('.reaction-button').first().click()
    const chip = page.locator('.reaction-chip').first()
    await expect(chip).toHaveAttribute('title', /^移除 /)
    await expect(chip).toHaveAttribute('aria-label', /^移除回应 /)

    await page.locator('.composer').first().evaluate((composer) => {
      const data = new DataTransfer()
      data.items.add(new File(['note'], 'note.txt', { type: 'text/plain' }))
      composer.dispatchEvent(new DragEvent('drop', { dataTransfer: data, bubbles: true, cancelable: true }))
    })
    await expect(page.locator('.file-chip-remove')).toHaveAttribute('aria-label', '移除 note.txt')

    await page.locator('.top-chrome-row .mac-chat-button').click()
    const actions = page.locator('.details-actions .mac-chat-button')
    await expect(actions).toHaveCount(2)
    expect(await actions.evaluateAll((buttons) => buttons.map((b) => [b.getAttribute('title'), b.getAttribute('aria-label')]))).toEqual([['创建模板', '创建模板'], ['关闭详情', '关闭详情']])
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await page.evaluate(() => localStorage.removeItem('appLanguage'))
    await page.reload()
    await browser.close()
  }
})
