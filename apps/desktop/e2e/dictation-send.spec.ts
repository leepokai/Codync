import { test, expect } from '@playwright/test'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { api, attach, echoBot, setPref } from './sandbox.ts'

// Send pressed while dictating after typed text (#446): what was said is transcribed first and goes in the same
// message, never dropped. Dictation runs on the OpenAI engine against a local OpenAI-compatible server, and the
// microphone is a tone, so nothing real is recorded or billed.
test('Send while dictating sends the typed text with the words said so far', async () => {
  const { browser, page } = await attach()
  const compatible = createServer((req, res) => {
    res.setHeader('Content-Type', 'application/json')
    req.resume()
    req.on('end', () => res.end(req.url?.includes('/audio/transcriptions') ? '{"text":"and the follow-up"}' : '{"data":[{"id":"whisper-1"}]}'))
  })
  await new Promise<void>((done) => compatible.listen(0, '127.0.0.1', done))
  const address = `http://127.0.0.1:${(compatible.address() as AddressInfo).port}`
  const name = `Dictation send e2e ${Date.now()}`
  let botId: string | undefined
  try {
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

    await page.locator('.composer textarea').fill('A')
    await page.getByRole('button', { name: 'Dictate' }).click()
    await expect(page.locator('.composer-recording')).toBeVisible()
    await page.waitForTimeout(1500) // Past the half second a recording needs to be sent.
    await page.locator('.composer-send').click()

    await expect(page.locator('.message.user').last()).toHaveText('A and the follow-up', { timeout: 20_000 })
    await expect(page.locator('.composer textarea')).toHaveValue('')
    await expect(page.locator('.composer-recording')).toHaveCount(0)
  } finally {
    await setPref(page, 'dictationEngine', 'device')
    await api(page, 'setVoiceKey', { provider: 'openai', key: '' }).catch(() => {})
    await api(page, 'setVoiceServer', { provider: 'openai', url: '' }).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId })
    await page.reload().catch(() => {}) // Brings back the real microphone and closes the tone.
    await browser.close()
    compatible.close()
  }
})
