import assert from 'node:assert/strict'
import { test } from 'node:test'
import { setMaxListeners } from 'node:events'
import type { AppLanguage } from './l10n.ts'
import type { TraySummary } from '../../shared/ipc.ts'

test('Additional languages persist their identifiers and reach the tray language contract', async (t) => {
  const originalWindow = Object.getOwnPropertyDescriptor(globalThis, 'window')
  const originalStorage = Object.getOwnPropertyDescriptor(globalThis, 'localStorage')
  t.after(() => {
    if (originalWindow) Object.defineProperty(globalThis, 'window', originalWindow)
    else Reflect.deleteProperty(globalThis, 'window')
    if (originalStorage) Object.defineProperty(globalThis, 'localStorage', originalStorage)
    else Reflect.deleteProperty(globalThis, 'localStorage')
  })
  const events = new EventTarget()
  setMaxListeners(0, events)
  const stored = new Map<string, string>()
  Object.defineProperty(globalThis, 'window', { configurable: true, value: events })
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: {
      getItem: (key: string) => stored.get(key) ?? null,
      setItem: (key: string, value: string) => { stored.set(key, value) },
    },
  })
  const { pref, prefs } = await import('./prefs.ts')
  const languages: TraySummary['language'][] = ['pt-BR', 'ru', 'it']
  assert.equal(prefs.appLanguage.get(), 'system')
  for (const language of languages) {
    prefs.appLanguage.set(language)
    assert.equal(stored.get('appLanguage'), JSON.stringify(language))
    assert.equal(pref<AppLanguage>('appLanguage', 'system').get(), language)
    const event = Object.assign(new Event('storage'), { key: 'appLanguage', newValue: JSON.stringify(language) })
    prefs.appLanguage.set('en')
    events.dispatchEvent(event)
    assert.equal(prefs.appLanguage.get(), language)
  }
})
