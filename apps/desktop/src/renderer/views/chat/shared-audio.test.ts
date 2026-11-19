import assert from 'node:assert/strict'
import { test } from 'node:test'
import { SharedAudio, type SharedAudioPlatform, type SoundEvents } from './shared-audio.ts'

/** A card's player whose fetch from the computer the test answers when it chooses. */
function setup() {
  const log: string[] = []
  const loads: ((data: Uint8Array | null) => void)[] = []
  let events!: SoundEvents
  const sound = {
    play: async () => { log.push('play'); events.playing(true) },
    pause: () => { log.push('pause'); events.playing(false) },
    seek: (seconds: number) => void log.push(`seek ${seconds}`),
    dispose: () => void log.push('dispose'),
  }
  const platform: SharedAudioPlatform = {
    load: () => new Promise<Uint8Array | null>((answer) => loads.push(answer)),
    open: (_, wired) => {
      log.push('open')
      events = wired
      return sound
    },
  }
  return { audio: new SharedAudio(platform, () => {}), log, loads, events: () => events, sound }
}

test('a second tap while the sound loads plays it once', async () => {
  const { audio, log, loads } = setup()
  const first = audio.toggle()
  await audio.toggle()
  assert.equal(audio.snapshot.phase, 'loading')
  assert.equal(loads.length, 1)
  loads[0](new Uint8Array([1]))
  await first
  assert.deepEqual(log, ['open', 'play'])
  assert.equal(audio.snapshot.phase, 'playing')
  await audio.toggle()
  assert.equal(audio.snapshot.phase, 'paused')
})

test('a card that goes while its sound loads plays nothing', async () => {
  const { audio, log, loads } = setup()
  const tap = audio.toggle()
  audio.dispose()
  loads[0](new Uint8Array([1]))
  await tap
  assert.deepEqual(log, [])
  assert.equal(audio.snapshot.phase, 'idle')
})

test('a sound that fails to load fails the card, and the next tap loads it again', async () => {
  const { audio, log, loads } = setup()
  const tap = audio.toggle()
  loads[0](null)
  await tap
  assert.equal(audio.snapshot.phase, 'failed')
  assert.equal(audio.failure, 'load')
  const retry = audio.toggle()
  assert.equal(loads.length, 2, 'the retry fetches again')
  loads[1](new Uint8Array([1]))
  await retry
  assert.deepEqual(log, ['open', 'play'])
  assert.equal(audio.snapshot.phase, 'playing')
})

test('a sound whose play() starts then rejects ends failed, not playing', async () => {
  const { audio, log, loads, events, sound } = setup()
  sound.play = async () => {
    log.push('play')
    events().playing(true) // <audio>.play() fires `play` before its promise rejects
    throw new Error('decode failed')
  }
  const tap = audio.toggle()
  loads[0](new Uint8Array([1]))
  await tap
  assert.deepEqual(log, ['open', 'play', 'pause', 'dispose'])
  assert.equal(audio.snapshot.phase, 'failed')
  assert.equal(audio.failure, 'playback')
  const retry = audio.toggle()
  assert.equal(loads.length, 2, 'the retry fetches again')
  loads[1](null)
  await retry
  assert.equal(audio.failure, 'load', 'each tap names the step it broke at')
})

test('a pause before play() settles keeps the loaded sound where it was', async () => {
  const { audio, log, loads, events, sound } = setup()
  sound.play = async () => {
    log.push('play')
    events().playing(true)
    events().time(3, 48)
    sound.pause() // <audio>.pause() while play() is pending rejects that play() with AbortError
    throw new DOMException('The play() request was interrupted by a call to pause().', 'AbortError')
  }
  const tap = audio.toggle()
  loads[0](new Uint8Array([1]))
  await tap
  assert.deepEqual(log, ['open', 'play', 'pause'])
  assert.deepEqual(audio.snapshot, { phase: 'paused', elapsed: 3, duration: 48 })
})

test('a loaded sound shows its clock and resets at the end', async () => {
  const { audio, loads, events } = setup()
  const tap = audio.toggle()
  loads[0](new Uint8Array([1]))
  await tap
  events().time(7, 48)
  assert.deepEqual(audio.snapshot, { phase: 'playing', elapsed: 7, duration: 48 })
  events().ended()
  assert.deepEqual(audio.snapshot, { phase: 'idle', elapsed: 0, duration: 48 })
})

test('a sound that fails mid-play drops its progress with the player', async () => {
  const { audio, loads, events } = setup()
  const tap = audio.toggle()
  loads[0](new Uint8Array([1]))
  await tap
  events().time(7, 48)
  events().failed()
  assert.deepEqual(audio.snapshot, { phase: 'failed', elapsed: 0, duration: 0 })
})

test('seeking holds the playhead to the sound and keeps it playing or paused', async () => {
  const { audio, log, loads, events } = setup()
  audio.seek(3)
  assert.deepEqual(log, [], 'nothing to seek before the sound loads')
  const tap = audio.toggle()
  loads[0](new Uint8Array([1]))
  await tap
  events().time(7, 48)
  audio.seek(20)
  audio.seek(-5)
  audio.seek(60)
  assert.deepEqual(log.slice(2), ['seek 20', 'seek 0', 'seek 48'])
  assert.deepEqual(audio.snapshot, { phase: 'playing', elapsed: 48, duration: 48 })
  await audio.toggle()
  audio.seek(12)
  assert.deepEqual(audio.snapshot, { phase: 'paused', elapsed: 12, duration: 48 })
})
