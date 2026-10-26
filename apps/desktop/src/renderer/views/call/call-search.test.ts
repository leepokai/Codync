import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import ts from 'typescript'

registerHooks({
  load(url, context, nextLoad) {
    if (url.endsWith('/call-link.ts')) {
      return { format: 'module', source: ts.transpileModule(readFileSync(new URL(url), 'utf8'), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText, shortCircuit: true }
    }
    return nextLoad(url, context)
  },
  resolve(specifier, context, nextResolve) {
    // CallLink only needs the timer from the store's sync module.
    if (specifier === '../../store/bot-sync') return { url: 'data:text/javascript,export const sleep = ms => new Promise(r => setTimeout(r, ms))', shortCircuit: true }
    if (specifier.startsWith('.') && context.parentURL && !/\.[a-z]+$/.test(specifier)) {
      const url = new URL(`${specifier}.ts`, context.parentURL)
      if (existsSync(url)) return nextResolve(url.href, context)
    }
    return nextResolve(specifier, context)
  },
})

const disk = new Map<string, string>()
Object.assign(globalThis, {
  localStorage: { getItem: (key: string) => disk.get(key) ?? null, setItem: (key: string, value: string) => disk.set(key, value) },
  window: { addEventListener() {} },
})
const { CallLink } = await import('./call-link.ts')
const { googleSearch } = await import('../settings/voice-settings.ts')

test('search opt-in is sent only for Gemini realtime, and needs the host acknowledgement', async () => {
  const requests: Record<string, unknown>[] = []
  let acknowledge = true
  let supported = true
  const client = { call: async (method: string, body: Record<string, unknown>) => {
    if (method === 'voiceStatus') return { providers: [], ...(supported ? { googleSearch: true } : {}) }
    assert.equal(method, 'startCall')
    requests.push(body)
    return { instructions: 'Relay', tools: [], ...(acknowledge ? { googleSearch: body.googleSearch } : {}) }
  } }
  const link = new CallLink({ ready: async () => client } as never, 'bot', 'call')
  googleSearch.set(false)
  await link.start('realtime', 'gemini')
  assert.equal(requests.at(-1)?.googleSearch, false)
  googleSearch.set(true)
  await link.start('realtime', 'gemini')
  assert.equal(requests.at(-1)?.googleSearch, true)
  await link.start('speech', 'gemini')
  assert.equal(requests.at(-1)?.googleSearch, false)
  await link.start('realtime', 'openai')
  assert.equal(requests.at(-1)?.googleSearch, false)
  acknowledge = false
  await assert.rejects(link.start('realtime', 'gemini'), /Update Codync/)
  supported = false
  await link.start('realtime', 'gemini')
  assert.equal(requests.at(-1)?.googleSearch, false)
  googleSearch.set(false)
  await link.start('realtime', 'gemini')
})
