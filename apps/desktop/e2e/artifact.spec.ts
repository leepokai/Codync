import { test, expect } from '@playwright/test'
import { join, resolve } from 'node:path'
import { api, attach } from './sandbox.ts'

// Publish through the real host/MCP path, so the viewer has the same card and reply target as a real artifact.
test('artifact keyboard navigation preserves renaming, menus and the message box', async () => {
  test.setTimeout(90_000)
  const { browser, page } = await attach()
  const oldViewer = page.locator('.artifact-layer')
  if (await oldViewer.count()) await oldViewer.getByRole('button', { name: 'Close artifact', exact: true }).click()
  const name = `Artifact keyboard ${Date.now()}`
  let botId: string | undefined
  try {
    const agent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/artifact_agent.py')
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: `python3 -X utf8 -u '${agent}'`, connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await api(page, 'send', { botId, text: 'make-artifact' })
    await page.locator('.roster-row', { hasText: name }).click()
    const card = page.locator('.artifact-card').last()
    await expect(card).toBeVisible()
    const layer = page.locator('.artifact-layer')
    await card.click()
    await expect(layer).toBeFocused()
    await page.keyboard.press('Tab')
    await expect(layer.getByRole('button', { name: 'Close artifact', exact: true })).toBeFocused()
    await page.keyboard.press('Escape')
    await expect(layer).toBeHidden()
    await expect(card).toBeVisible()

    await card.click()
    await layer.getByTitle('Rename artifact').click()
    const title = layer.getByRole('textbox', { name: 'Artifact title', exact: true })
    await title.fill('Canceled title')
    await title.press('Escape')
    await expect(layer).toBeVisible()
    await expect(layer.getByTitle('Rename artifact')).toContainText('Weekly report')
    await expect(layer).toBeFocused()
    await page.keyboard.press('Escape')
    await expect(layer).toBeHidden()

    await card.click()
    await layer.getByTitle('Rename artifact').click()
    await title.fill('Renamed report')
    await title.press('Enter')
    await expect(layer.getByTitle('Rename artifact')).toContainText('Renamed report')
    await expect(layer).toBeFocused()
    await layer.getByTitle('Rename artifact').click()
    const composer = layer.locator('.composer textarea')
    await composer.click()
    await expect(composer).toBeFocused()
    await composer.fill('Draft stays here')
    await composer.press('Escape')
    await expect(layer).toBeVisible()
    await expect(composer).toHaveValue('Draft stays here')

    await layer.getByRole('button', { name: 'Share artifact' }).click()
    await expect(page.getByRole('menuitem', { name: 'Save as HTML' })).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(page.getByRole('menuitem', { name: 'Save as HTML' })).toBeHidden()
    await expect(layer).toBeVisible()
    await layer.locator('iframe').contentFrame().locator('h1').click()
    await layer.getByRole('button', { name: 'Close artifact', exact: true }).click()
    await expect(layer).toBeHidden()
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
