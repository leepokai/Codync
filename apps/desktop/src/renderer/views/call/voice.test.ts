import assert from 'node:assert/strict'
import { existsSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'

// The renderer imports without extensions, as the bundler resolves them.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('.') && context.parentURL && !/\.[a-z]+$/.test(specifier)) {
      const url = new URL(`${specifier}.ts`, context.parentURL)
      if (existsSync(url)) return nextResolve(url.href, context)
    }
    return nextResolve(specifier, context)
  },
})

test('a call takes another engine only when what its engine is built from changes', async () => {
  const disk = new Map<string, string>()
  Object.assign(globalThis, {
    localStorage: { getItem: (key: string) => disk.get(key) ?? null, setItem: (key: string, value: string) => disk.set(key, value) },
    window: { addEventListener() {} },
  })
  const { VoiceSettings } = await import('./voice.ts')
  const settings = await import('../settings/voice-settings.ts')

  const device = VoiceSettings.engineChoice(null)
  assert.equal(VoiceSettings.engineChoice(null), device)
  settings.language.set('ja-JP')
  assert.notEqual(VoiceSettings.engineChoice(null), device)
  assert.notEqual(VoiceSettings.engineChoice('gemini'), VoiceSettings.engineChoice('openai'))

  const seen = [VoiceSettings.engineChoice('gemini')]
  for (const change of [
    () => settings.saveModel('gemini', 'realtime', 'live-9'),
    () => settings.saveVoice('gemini', 'Puck'),
    () => settings.mode('gemini').set('speech'),
    () => settings.saveModel('gemini', 'transcribe', 'flash-9'),
    () => settings.saveModel('gemini', 'speech', 'tts-9'),
  ]) {
    change()
    const choice = VoiceSettings.engineChoice('gemini')
    assert.equal(VoiceSettings.engineChoice('gemini'), choice)
    assert.ok(!seen.includes(choice), choice)
    seen.push(choice)
  }
})


test('Windows and Linux retain automatic call fallback until a provider is explicitly picked', async () => {
  const { VoiceSettings } = await import('./voice.ts')
  const settings = await import('../settings/voice-settings.ts')
  for (const platform of ['win32', 'linux']) {
    Object.assign(window, { codync: { platform } })
    settings.engine.set('device')
    // The picker offers a supported choice, but startup still enters configured-provider fallback.
    assert.equal(settings.engine.get(), 'openai')
    assert.equal(VoiceSettings.provider(), null)
    settings.engine.set('gemini')
    assert.equal(VoiceSettings.provider(), 'gemini')
    assert.equal(settings.engine.get(), 'gemini')
  }
})

/** A transport that gives up when its budget runs out, and a computer whose speech server answers just inside its two minutes. */
function slowHost(answer: unknown) {
  return {
    call: (_method: string, _body: unknown, timeoutMs: number) =>
      new Promise((resolve, reject) => {
        setTimeout(() => reject(new Error("Can't reach your computer.")), timeoutMs)
        setTimeout(() => resolve(answer), 121_000)
      }),
  }
}

test("linking a speech server waits out the computer's own two minutes for its test transcription", async (t) => {
  const settings = await import('../settings/voice-settings.ts')
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const slow = slowHost({ ready: true, message: null, server: 'http://127.0.0.1:8080/inference' }) as unknown as Parameters<typeof settings.setSpeechServer>[0]
  const linking = settings.setSpeechServer(slow, 'http://127.0.0.1:8080/inference')
  t.mock.timers.tick(121_000)
  assert.equal((await linking).ready, true)
})

test("a dictated piece on the computer's own speech server waits out its two minutes too", async (t) => {
  const voice = await import('./voice.ts')
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const slow = slowHost({ text: 'slow but heard' }) as unknown as Parameters<typeof voice.transcribeOnHost>[0]
  const piece = voice.transcribeOnHost(slow, 'local', '', new Uint8Array())
  t.mock.timers.tick(121_000)
  assert.equal(await piece, 'slow but heard')
})
