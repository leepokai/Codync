import assert from 'node:assert/strict'
import { existsSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { SpeechEvent } from '@shared/ipc'

// The on-device call driven by a scripted speech helper: what it sends after the recognizer starts over at a pause.

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

let listener: ((e: SpeechEvent) => void) | null = null
const disk = new Map<string, string>()
Object.assign(globalThis, {
  localStorage: { getItem: (key: string) => disk.get(key) ?? null, setItem: (key: string, value: string) => disk.set(key, value) },
  speechSynthesis: { getVoices: () => [], cancel() {} },
  window: {
    addEventListener() {},
    codync: {
      speech: {
        available: true,
        start() {},
        stop() {},
        onEvent: (l: (e: SpeechEvent) => void) => {
          listener = l
          return () => void (listener = null)
        },
      },
    },
  },
})
const { OnDeviceEngine } = await import('./OnDeviceEngine.ts')

test('words after a pause are sent after the words before it, and the next turn starts empty', async (t) => {
  t.mock.timers.enable({ apis: ['Date', 'setTimeout'] })
  const sent: string[] = []
  const engine = new OnDeviceEngine((text) => sent.push(text))
  await engine.start()
  const say = (e: SpeechEvent) => listener?.(e)
  say({ type: 'started' })
  say({ type: 'partial', text: 'I fixed the parser' })
  // A pause the recognizer starts over at, shorter than the call's (1.5 s by default).
  t.mock.timers.tick(1200)
  say({ type: 'partial', text: 'now' })
  say({ type: 'final', text: 'now run the tests' })
  assert.deepEqual(sent, ['I fixed the parser now run the tests'])
  say({ type: 'started' })
  say({ type: 'final', text: 'thanks' })
  assert.deepEqual(sent, ['I fixed the parser now run the tests', 'thanks'])
  engine.end()
})
