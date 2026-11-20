import { test, expect, type Page } from '@playwright/test'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { api, attach, echoBot, openGeneralSettings, pick, setPref } from './sandbox.ts'

// The voice input shortcut's tap after its double tap (#385): Settings → General picks Send or Stop without
// sending, and on Stop the words stay in the composer. The ⌘ key watcher lives in the main process, so the taps go
// in where the chat window takes them (`queueVoiceKey`); dictation runs on the OpenAI engine against a local server
// with a tone for the microphone, as in dictation-send.spec.ts.

/** Hands the composer a tap of the shortcut, through the module instance the window itself loaded. */
async function voiceKey(page: Page, key: 'double' | 'tap') {
  await page.evaluate(async (key) => {
    const url = performance.getEntriesByType('resource').map((e) => e.name).find((n) => n.includes('/chat/voice-shortcut-keys.ts'))
    if (!url) throw new Error('voice-shortcut-keys.ts is not loaded')
    const { queueVoiceKey } = (await import(url)) as { queueVoiceKey: (key: string) => void }
    queueVoiceKey(key)
  }, key)
}

async function dictateByShortcut(page: Page) {
  // A double tap reaches the window as the key watcher reports it (CommandTaps): its first tap, then the double.
  await voiceKey(page, 'tap')
  await voiceKey(page, 'double')
  await expect(page.locator('.composer-recording')).toBeVisible()
  await page.waitForTimeout(1500) // Past the half second a recording needs to be sent.
  await voiceKey(page, 'tap')
  await expect(page.locator('.composer-recording')).toHaveCount(0, { timeout: 20_000 })
}

test('Tap again to: Stop without sending keeps the words in the box, Send sends them', async () => {
  const { browser, page } = await attach()
  test.skip(!(await page.evaluate(() => window.codync.speech.available)), 'The voice input shortcut needs on-device speech (macOS).')
  const compatible = createServer((req, res) => {
    res.setHeader('Content-Type', 'application/json')
    req.resume()
    req.on('end', () => res.end(req.url?.includes('/audio/transcriptions') ? '{"text":"check the parser"}' : '{"data":[{"id":"whisper-1"}]}'))
  })
  await new Promise<void>((done) => compatible.listen(0, '127.0.0.1', done))
  const address = `http://127.0.0.1:${(compatible.address() as AddressInfo).port}`
  const name = `Voice tap e2e ${Date.now()}`
  let botId: string | undefined
  const before = await page.evaluate(() => window.codync.speech.shortcut())
  try {
    await page.evaluate(() => window.codync.speech.setShortcut('off'))
    await setPref(page, 'voiceShortcutTap', 'send')
    await page.reload()

    // Settings: the row shows only while the shortcut is on, and its choice changes the shortcut's detail.
    await openGeneralSettings(page)
    const shortcutRow = page.locator('.value-row', { hasText: 'Voice input shortcut' })
    await expect(shortcutRow).toBeVisible()
    await expect(page.locator('.value-row', { hasText: 'Tap again to' })).toHaveCount(0)
    await pick(page, 'Off', 'Right ⌘ twice')
    await expect(page.locator('.value-row', { hasText: 'Tap again to' })).toBeVisible()
    await expect(shortcutRow).toContainText('tap the key once more to send.')
    await pick(page, 'Send', 'Stop without sending')
    await expect(shortcutRow).toContainText('tap the key once more to stop without sending.')
    expect(await page.evaluate(() => localStorage.getItem('voiceShortcutTap'))).toBe('"stop"')
    await page.locator('.settings-close button').click()

    await api(page, 'setVoiceServer', { provider: 'openai', url: address })
    await api(page, 'setVoiceKey', { provider: 'openai', key: 'sk-e2e-local' })
    await setPref(page, 'dictationEngine', 'openai')
    await page.evaluate(() => {
      navigator.mediaDevices.getUserMedia = async () => {
        const ctx = new AudioContext()
        const tone = ctx.createOscillator()
        const out = ctx.createMediaStreamDestination()
        tone.connect(out)
        tone.start()
        return out.stream
      }
    })
    botId = (await echoBot(page, name)).id
    await page.locator('.roster-row', { hasText: name }).click()
    await page.locator('.composer textarea').fill('Please')

    // Stop: the words land in the box and nothing is sent.
    await dictateByShortcut(page)
    await expect(page.locator('.composer textarea')).toHaveValue('Please check the parser')
    await page.waitForTimeout(500)
    await expect(page.locator('.message.user')).toHaveCount(0)

    // Send: the same tap sends what was said.
    await setPref(page, 'voiceShortcutTap', 'send')
    await page.locator('.composer textarea').fill('Also')
    await dictateByShortcut(page)
    await expect(page.locator('.message.user').last()).toHaveText('Also check the parser', { timeout: 20_000 })
    await expect(page.locator('.composer textarea')).toHaveValue('')
  } finally {
    await setPref(page, 'voiceShortcutTap', 'send')
    await setPref(page, 'dictationEngine', 'device')
    await page.evaluate((s) => window.codync.speech.setShortcut(s), before).catch(() => {})
    await api(page, 'setVoiceKey', { provider: 'openai', key: '' }).catch(() => {})
    await api(page, 'setVoiceServer', { provider: 'openai', url: '' }).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId })
    await page.reload().catch(() => {})
    await browser.close()
    compatible.close()
  }
})
