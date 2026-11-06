import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { MemoryStorage } from '../lib/memory-storage.ts'
import { AnalyticsQueue } from '../lib/analytics-queue.ts'

// The demo runs on sample data in memory. These tests fail if anything it is made of reaches for the network, the disk or
// the real host: the files that make up the demo are read as text and must not contain the ways to do so.

const FILES = ['fake-host.ts', 'story.ts', 'demo-transport.ts', '../store/demo-mode.ts', '../lib/memory-storage.ts']
const FORBIDDEN: [string, RegExp][] = [
  ['the network (fetch)', /\bfetch\s*\(/],
  ['the network (XMLHttpRequest)', /XMLHttpRequest/],
  ['the network (WebSocket)', /\bWebSocket\b/],
  ['the network (EventSource)', /\bEventSource\b/],
  ['the device (localStorage)', /\blocalStorage\b/],
  ['the device (sessionStorage)', /\bsessionStorage\b/],
  ['the device (indexedDB)', /\bindexedDB\b/],
  ['the device (node:fs)', /node:fs|from 'fs'/],
  ['the real host (window.codync.host)', /window\.codync\.host/],
  ['the real host (LoopbackTransport)', /LoopbackTransport/],
  ['the browser (openExternal)', /openExternal/],
]

test('the demo is made of files that touch neither the network, the disk nor the real host', () => {
  for (const file of FILES) {
    const source = readFileSync(new URL(file, import.meta.url), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '')
    for (const [what, pattern] of FORBIDDEN) assert.ok(!pattern.test(source), `${file} reaches for ${what}`)
  }
})

test('the demo model keeps its cache and drafts in memory', () => {
  const source = readFileSync(new URL('../store/bot-mirror.ts', import.meta.url), 'utf8')
  assert.match(source, /if \(this\.volatile\) return/, 'loading the cache is skipped')
  assert.match(source, /if \(this\.retired \|\| this\.volatile\) return/, 'saving the cache is skipped')
  assert.match(source, /volatile \? new MemoryStorage\(\) : localStorage/, 'drafts go to memory')
  const model = readFileSync(new URL('../store/app-model.ts', import.meta.url), 'utf8')
  assert.match(model, /if \(!this\.demo\) this\.connectAnalytics|if \(!this\.demo\) account\.subscribe|if \(!demo\) this\.connectAnalytics/, 'the demo is not connected to analytics or the account')
})

test('a memory storage holds values only until it is dropped', () => {
  const storage = new MemoryStorage()
  assert.equal(storage.getItem('k'), null)
  storage.setItem('k', 'v')
  assert.equal(storage.getItem('k'), 'v')
  storage.removeItem('k')
  assert.equal(storage.getItem('k'), null)
  assert.equal(new MemoryStorage().getItem('k'), null, 'another one starts empty')
})

test('while quiet, the analytics queue counts nothing and sends nothing', async () => {
  const sent: string[] = []
  const queue = new AnalyticsQueue()
  queue.connect(async (event) => { sent.push(event.event) })
  queue.setQuiet(true)
  queue.track({ event: 'screen_viewed', screen: 'memory' })
  await new Promise((resolve) => setTimeout(resolve, 10))
  assert.deepEqual(sent, [])
  queue.setQuiet(false)
  queue.track({ event: 'screen_viewed', screen: 'memory' })
  await new Promise((resolve) => setTimeout(resolve, 10))
  assert.deepEqual(sent, ['screen_viewed'])
})
