import { test, expect } from '@playwright/test'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { api, attach, echoBot, setPref, steerBot } from './sandbox.ts'

// The island keeps its recorded words while they wait for the host: no idle mic can cancel the transcription.
test('Island dictation waits for its transcript without offering another recording', async () => {
  const { browser, page } = await attach()
  let transcribed = false
  let release = () => {}
  const gate = new Promise<void>((done) => (release = done))
  const compatible = createServer((req, res) => {
    res.setHeader('Content-Type', 'application/json')
    req.resume()
    req.on('end', async () => {
      if (req.url?.includes('/audio/transcriptions')) {
        transcribed = true
        await gate
        res.end('{"text":"and the follow-up"}')
      } else {
        res.end('{"data":[{"id":"whisper-1"}]}')
      }
    })
  })
  await new Promise<void>((done) => compatible.listen(0, '127.0.0.1', done))
  const address = `http://127.0.0.1:${(compatible.address() as AddressInfo).port}`
  const name = `Island dictation e2e ${Date.now()}`
  let botId: string | undefined
  try {
    await setPref(page, 'appLanguage', 'en')
    await api(page, 'setVoiceServer', { provider: 'openai', url: address })
    await api(page, 'setVoiceKey', { provider: 'openai', key: 'sk-e2e-local' })
    await setPref(page, 'dictationEngine', 'openai')
    botId = (await steerBot(page, name, 60)).id
    await api(page, 'send', { botId, text: 'Hold this turn' })
    await page.evaluate(() => window.codync.island.setEnabled(true))
    await expect.poll(() => browser.contexts().flatMap((context) => context.pages())
      .find((p) => new URL(p.url()).searchParams.get('window') === 'island')).toBeTruthy()
    const islandPage = browser.contexts().flatMap((context) => context.pages())
      .find((p) => new URL(p.url()).searchParams.get('window') === 'island')!
    await islandPage.evaluate(() => {
      navigator.mediaDevices.getUserMedia = async () => {
        const ctx = new AudioContext()
        const tone = ctx.createOscillator()
        const out = ctx.createMediaStreamDestination()
        tone.connect(out)
        tone.start()
        return out.stream
      }
    })
    await islandPage.getByRole('button', { name: 'Show bots' }).click()
    await islandPage.getByRole('button', { name: 'Message a bot' }).click()
    await islandPage.locator('.island-field input').fill('A')
    await islandPage.getByRole('button', { name: 'Dictate' }).click()
    await expect(islandPage.locator('.recording-pill')).toBeVisible()
    await islandPage.waitForTimeout(1500)
    await islandPage.locator('.recording-stop').click()

    await expect(islandPage.locator('.recording-pill')).toHaveText('Transcribing…')
    await expect(islandPage.getByRole('button', { name: 'Dictate' })).toHaveCount(0)
    await expect.poll(() => transcribed).toBe(true)
    release()
    await expect(islandPage.locator('.island-field input')).toHaveValue('A and the follow-up')
    await expect(islandPage.locator('.recording-pill')).toHaveCount(0)
  } finally {
    release()
    await page.evaluate(() => window.codync.island.setEnabled(false)).catch(() => {})
    await setPref(page, 'dictationEngine', 'device')
    await api(page, 'setVoiceKey', { provider: 'openai', key: '' }).catch(() => {})
    await api(page, 'setVoiceServer', { provider: 'openai', url: '' }).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId })
    await browser.close()
    compatible.close()
  }
})

// Send pressed while the words are still on their way waits for the transcript and sends one message: the
// dictated words alone when nothing was typed, and the typed text with them when it was.
test('Island Send during dictation sends one message with the typed text and the words', async () => {
  const { browser, page } = await attach()
  let release = () => {}
  let gate = new Promise<void>((done) => (release = done))
  const compatible = createServer((req, res) => {
    res.setHeader('Content-Type', 'application/json')
    req.resume()
    req.on('end', async () => {
      if (req.url?.includes('/audio/transcriptions')) {
        await gate
        res.end('{"text":"and the follow-up"}')
      } else {
        res.end('{"data":[{"id":"whisper-1"}]}')
      }
    })
  })
  await new Promise<void>((done) => compatible.listen(0, '127.0.0.1', done))
  const address = `http://127.0.0.1:${(compatible.address() as AddressInfo).port}`
  const name = `Island dictation send e2e ${Date.now()}`
  let botId: string | undefined
  try {
    await setPref(page, 'appLanguage', 'en')
    const continueWithoutAccount = page.getByRole('button', { name: /^Continue without an account/ })
    if (await continueWithoutAccount.isVisible()) await continueWithoutAccount.click()
    await api(page, 'setVoiceServer', { provider: 'openai', url: address })
    await api(page, 'setVoiceKey', { provider: 'openai', key: 'sk-e2e-local' })
    await setPref(page, 'dictationEngine', 'openai')
    botId = (await echoBot(page, name)).id
    await page.locator('.roster-row', { hasText: name }).click()
    await page.evaluate(() => window.codync.island.setEnabled(true))
    await expect.poll(() => browser.contexts().flatMap((context) => context.pages())
      .find((p) => new URL(p.url()).searchParams.get('window') === 'island')).toBeTruthy()
    const islandPage = browser.contexts().flatMap((context) => context.pages())
      .find((p) => new URL(p.url()).searchParams.get('window') === 'island')!
    await islandPage.evaluate(() => {
      navigator.mediaDevices.getUserMedia = async () => {
        const ctx = new AudioContext()
        const tone = ctx.createOscillator()
        const out = ctx.createMediaStreamDestination()
        tone.connect(out)
        tone.start()
        return out.stream
      }
    })

    // An empty field: Send while still recording waits for the transcript, then sends only the words.
    // Bot idle: the island is hidden (opacity 0) — a direct click event opens it like a user's click.
    await islandPage.getByRole('button', { name: 'Show bots' }).dispatchEvent('click')
    await islandPage.locator('button[aria-label="Message a bot"]').click()
    await islandPage.getByRole('button', { name: 'Dictate' }).click()
    await expect(islandPage.locator('.recording-pill')).toBeVisible()
    await islandPage.waitForTimeout(1500)
    await islandPage.locator('.island-send').click()
    release()
    await expect(page.locator('.message.user')).toHaveCount(1, { timeout: 20_000 })
    await expect(page.locator('.message.user').last()).toHaveText('and the follow-up')

    // Typed text: Send while transcribing joins the words to what was typed in the same message.
    gate = new Promise<void>((done) => (release = done))
    await islandPage.getByRole('button', { name: 'Show bots' }).dispatchEvent('click')
    await islandPage.locator('button[aria-label="Message a bot"]').click()
    await islandPage.locator('.island-field input').fill('A')
    await islandPage.getByRole('button', { name: 'Dictate' }).click()
    await expect(islandPage.locator('.recording-pill')).toBeVisible()
    await islandPage.waitForTimeout(1500)
    await islandPage.locator('.recording-stop').click()
    await expect(islandPage.locator('.recording-pill')).toHaveText('Transcribing…')
    await islandPage.locator('.island-send').click()
    release()
    await expect(page.locator('.message.user')).toHaveCount(2, { timeout: 20_000 })
    await expect(page.locator('.message.user').last()).toHaveText('A and the follow-up')
  } finally {
    release()
    await page.evaluate(() => window.codync.island.setEnabled(false)).catch(() => {})
    await setPref(page, 'dictationEngine', 'device')
    await api(page, 'setVoiceKey', { provider: 'openai', key: '' }).catch(() => {})
    await api(page, 'setVoiceServer', { provider: 'openai', url: '' }).catch(() => {})
    if (botId) await api(page, 'deleteBot', { botId })
    await browser.close()
    compatible.close()
  }
})
