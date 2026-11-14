import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { SharedFile } from '@shared/models'
import type { DownloadState } from '../../store/file-downloads.ts'
import { byteCount, cardActions, cardDetail, cardKeyAction, clickRunsFirstAction, clock } from './file-card.ts'
import type { AudioSnapshot } from './shared-audio.ts'

const file = (size: number): SharedFile => ({ id: 'f1', name: 'venue-floor-plan.pdf', size, sha256: 'a'.repeat(64) })
const audio = (phase: AudioSnapshot['phase'], elapsed = 0, duration = 0): AudioSnapshot => ({ phase, elapsed, duration })

test('byteCount matches ByteCountFormatter(.file): decimal units, no needless zeros', () => {
  const expected: [number, string][] = [
    [0, 'Zero KB'], [1, '1 byte'], [43, '43 bytes'], [999, '999 bytes'],
    [1000, '1 KB'], [1499, '1 KB'], [1500, '2 KB'], [47600, '48 KB'], [64400, '64 KB'], [380030, '380 KB'],
    [999500, '1 MB'], [1049999, '1 MB'], [1050000, '1.1 MB'], [1234567, '1.2 MB'], [12345678, '12.3 MB'],
    [1000000000, '1 GB'], [1234567890, '1.23 GB'],
  ]
  for (const [bytes, text] of expected) assert.equal(byteCount(bytes), text, `${bytes}`)
})

test('clock writes m:ss, and h:mm:ss past an hour', () => {
  assert.equal(clock(0), '0:00')
  assert.equal(clock(7.9), '0:07')
  assert.equal(clock(48), '0:48')
  assert.equal(clock(600), '10:00')
  assert.equal(clock(3725), '1:02:05')
})

test('a card that could not play says so before anything else', () => {
  const downloading: DownloadState = { kind: 'downloading', received: 500 }
  assert.equal(cardDetail(downloading, file(1000), audio('failed')), `Couldn't play. Click to retry.`)
})

test('a card that could not play writes its retry in the saved language', (t) => {
  // trNow reads `appLanguage` from localStorage on every call; stub it and put it back after.
  const original = Object.getOwnPropertyDescriptor(globalThis, 'localStorage')
  t.after(() => {
    if (original) Object.defineProperty(globalThis, 'localStorage', original)
    else Reflect.deleteProperty(globalThis, 'localStorage')
  })
  let lang = 'fr'
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    writable: true,
    value: { getItem: (key: string) => (key === 'appLanguage' ? JSON.stringify(lang) : null) },
  })

  const failed = audio('failed')
  const french = cardDetail(undefined, file(1000), failed)
  assert.equal(french, 'Lecture impossible. Cliquez pour réessayer.')
  assert.notEqual(french, "Couldn't play. Click to retry.")

  lang = 'en'
  assert.equal(cardDetail(undefined, file(1000), failed), "Couldn't play. Click to retry.")
})

test('a downloading or failed download beats the sound clock', () => {
  const downloading: DownloadState = { kind: 'downloading', received: 500 }
  const failed: DownloadState = { kind: 'failed', message: 'No space left.' }
  assert.equal(cardDetail(downloading, file(1000), audio('playing', 7, 48)), '500 bytes of 1 KB')
  assert.equal(cardDetail(failed, file(1000), audio('playing', 7, 48)), 'No space left.')
})

test('a loaded sound shows elapsed of duration while playing, paused or done', () => {
  for (const phase of ['playing', 'paused', 'idle'] as const) {
    assert.equal(cardDetail(undefined, file(64400), audio(phase, 7, 48)), '0:07 / 0:48')
  }
})

test('a card without a loaded sound or download keeps the file detail', () => {
  assert.equal(cardDetail(undefined, file(380030)), '380 KB')
  assert.equal(cardDetail(undefined, file(380030), audio('idle')), '380 KB')
  assert.equal(cardDetail(undefined, file(380030), audio('loading')), '380 KB')
  const saved: DownloadState = { kind: 'saved' }
  assert.equal(cardDetail(saved, file(380030), audio('idle')), '380 KB · Saved')
})

test("a card's first action is a voice note's play (also after a failure), then a picture's view, then the download", () => {
  const noop = () => {}
  const titles = (args: Partial<Parameters<typeof cardActions>[0]>) => cardActions({ state: undefined, download: noop, ...args }).map((a) => a.title)
  assert.deepEqual(titles({ audio: audio('failed'), toggle: noop }), ['Play', 'Download'])
  assert.deepEqual(titles({ audio: audio('playing', 7, 48), toggle: noop }), ['Pause', 'Download'])
  assert.deepEqual(titles({ view: noop, state: { kind: 'downloading', received: 1 } }), ['View', 'Cancel download'])
  assert.deepEqual(titles({}), ['Download'])
})

test("picking Download from a voice note's menu by keyboard never plays or moves it", () => {
  const note = audio('paused', 7, 48)
  const card = (key: string, shiftKey: boolean, menuOpen: boolean) => cardKeyAction({ key, shiftKey }, menuOpen, note)?.kind
  assert.equal(card('F10', true, false), 'menu')
  for (const key of ['ArrowDown', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Enter']) assert.equal(card(key, false, true), undefined, key)
  assert.equal(card(' ', false, true), undefined)
  assert.equal(card('Enter', false, false), 'first')
  assert.equal(card(' ', false, false), 'first')
  assert.equal(card('ContextMenu', false, false), 'menu')
  assert.equal(card('F10', false, false), undefined)
})

test('arrow keys move a loaded voice note five seconds; other keys and an unloaded sound move nothing', () => {
  const seek = (key: string, note?: AudioSnapshot) => {
    const action = cardKeyAction({ key, shiftKey: false }, false, note)
    return action?.kind === 'seek' ? action.seconds : undefined
  }
  assert.equal(seek('ArrowRight', audio('paused', 7, 48)), 12)
  assert.equal(seek('ArrowLeft', audio('playing', 7, 48)), 2)
  assert.equal(seek('ArrowUp', audio('paused', 7, 48)), undefined)
  assert.equal(seek('ArrowRight', audio('idle')), undefined)
  assert.equal(seek('ArrowRight', undefined), undefined)
})

test("a click on a card's name or caption runs its first action; one on its own buttons or bar only theirs", () => {
  // A click target inside the card, under an element with this tag (or none).
  const inside = (tag: string | null) => ({ closest: (selector: string) => (tag && selector.split(', ').includes(tag) ? {} : null) })
  assert.equal(clickRunsFirstAction(inside(null)), true)
  assert.equal(clickRunsFirstAction(inside('button')), false)
  assert.equal(clickRunsFirstAction(inside('input')), false)
})
