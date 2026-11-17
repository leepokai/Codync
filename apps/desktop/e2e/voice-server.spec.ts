import { test, expect, type Page } from '@playwright/test'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { api, attach, openVoiceSettings, pick } from './sandbox.ts'

const prefs = ['callEngine', 'callMode.openai', 'callMode.gemini', 'callServer.openai']
type Status = { providers: { provider: string; configured: boolean; server?: string }[] }
const openai = async (page: Page) => (await api<Status>(page, 'voiceStatus', {})).providers.find((p) => p.provider === 'openai')
const server = async (page: Page) => (await openai(page))?.server

// The OpenAI engine's Server row (docs/features/voice-call.md): an address saved on the computer shows on the row,
// leaves Speech as the only mode for that engine alone, and clearing it goes back to OpenAI and the mode picked
// before, without the key saved with the server. A refused address keeps the saved one.
test("the OpenAI engine's Server row saves an address on the computer, and Speech is then the only mode", async () => {
  const { browser, page } = await attach()
  const saved = await page.evaluate((keys) => keys.map((k) => localStorage.getItem(k)), prefs)
  const compatible = createServer((_, res) => res.setHeader('Content-Type', 'application/json').end('{"data":[{"id":"whisper-1"}]}'))
  await new Promise<void>((done) => compatible.listen(0, '127.0.0.1', done))
  const address = `http://127.0.0.1:${(compatible.address() as AddressInfo).port}`
  try {
    expect(await server(page)).toBe('')
    await page.evaluate(() => {
      localStorage.setItem('callEngine', JSON.stringify('openai'))
      localStorage.setItem('callMode.openai', JSON.stringify('realtime'))
      localStorage.setItem('callMode.gemini', JSON.stringify('realtime'))
    })
    await page.reload()
    await openVoiceSettings(page)
    const field = page.getByRole('textbox', { name: 'Server' })
    await expect(field).toHaveAttribute('placeholder', 'api.openai.com')
    await expect(page.locator('.choice-picker[title="Realtime"]')).toBeVisible()

    // Off this computer, a key would cross the network in cleartext: refused, and the field goes back.
    await field.fill('http://192.168.1.20:8890')
    await field.press('Enter')
    await expect(page.getByText('The server must be an https URL', { exact: false })).toBeVisible()
    await expect(field).toHaveValue('')
    expect(await server(page)).toBe('')

    await field.fill(`${address}/v1`)
    await field.press('Enter')
    await expect(page.getByRole('textbox', { name: 'Server' })).toHaveValue(address)
    expect(await server(page)).toBe(address)
    await expect(page.getByText('Your own server works in Speech mode only.')).toBeVisible()
    await expect(page.locator('.choice-picker[title="Realtime"]')).toHaveCount(0)

    // Only the OpenAI engine: Gemini keeps its Realtime, and the mode picked for OpenAI isn't overwritten.
    await pick(page, 'OpenAI', 'Gemini')
    await expect(page.locator('.choice-picker[title="Realtime"]')).toBeVisible()
    await expect(page.getByText('Your own server works in Speech mode only.')).toHaveCount(0)
    await pick(page, 'Gemini', 'OpenAI')
    await expect(page.getByText('Your own server works in Speech mode only.')).toBeVisible()
    expect(await page.evaluate(() => ['callMode.openai', 'callMode.gemini'].map((k) => localStorage.getItem(k)))).toEqual(['"realtime"', '"realtime"'])

    // Kept on the computer: a reload shows it again, with a key the server took.
    await api(page, 'setVoiceKey', { provider: 'openai', key: 'local-key' })
    await page.reload()
    await openVoiceSettings(page)
    await expect(page.getByRole('textbox', { name: 'Server' })).toHaveValue(address)
    await expect(page.getByText('Key saved', { exact: true })).toBeVisible()

    // Cleared: back to OpenAI, and that key goes with the server, so it never reaches OpenAI.
    await page.getByRole('textbox', { name: 'Server' }).fill('')
    await page.getByRole('textbox', { name: 'Server' }).press('Enter')
    await expect(page.locator('.choice-picker[title="Realtime"]')).toBeVisible()
    await expect(page.getByText('Key saved', { exact: true })).toHaveCount(0)
    expect(await openai(page)).toMatchObject({ server: '', configured: false })
  } finally {
    await api(page, 'setVoiceServer', { provider: 'openai', url: '' })
    await api(page, 'setVoiceKey', { provider: 'openai', key: '' })
    await page.evaluate(([keys, values]) => keys.forEach((k, i) => {
      if (values[i] === null) localStorage.removeItem(k)
      else localStorage.setItem(k, values[i]!)
    }), [prefs, saved] as const)
    await page.reload() // Closes Settings, which would cover the next spec.
    await browser.close()
    compatible.close()
  }
})
