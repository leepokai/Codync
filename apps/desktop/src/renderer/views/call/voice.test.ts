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
