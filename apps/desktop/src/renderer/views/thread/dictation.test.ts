import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { SpeechEvent } from '../../../shared/ipc.ts'
import { Dictation, type DictationPlatform } from './dictation.ts'
import { MIN_SAMPLES, PIECE_SAMPLES } from './dictation-pieces.ts'

/** A stub platform: a speech helper driven by `say`, a microphone fed by `hear`, transcripts on demand. */
function setup(over: Partial<DictationPlatform> = {}) {
  let listener: ((e: SpeechEvent) => void) | null = null
  let feed: ((pcm: Int16Array, level: number) => void) | null = null
  const log: string[] = []
  const pending: { piece: Int16Array; answer: (text: string) => void; fail: (e: Error) => void }[] = []
  const platform: DictationPlatform = {
    speech: {
      start: () => void log.push('speech.start'),
      stop: () => void log.push('speech.stop'),
      onEvent: (l) => {
        listener = l
        return () => void (listener = null)
      },
    },
    noSpeech: 'No speech here.',
    host: true,
    language: () => null,
    provider: () => null,
    checkKey: async () => true,
    configureKey: (provider) => void log.push(`configure ${provider}`),
    microphone: async (out) => {
      feed = out
      log.push('mic.open')
      return () => void log.push('mic.close')
    },
    transcribe: (_, piece) => new Promise((answer, fail) => pending.push({ piece, answer, fail })),
    count: (seconds) => void log.push(`count ${seconds}`),
    report: (message) => void log.push(`report ${message}`),
    ...over,
  }
  let box = 'Fix the bug'
  const dictation = new Dictation((change) => (box = change(box)), platform)
  return {
    dictation,
    log,
    pending,
    box: () => box,
    type: (text: string) => void (box += text),
    say: (e: SpeechEvent) => listener?.(e),
    hear: (samples: number) => feed?.(new Int16Array(samples), 0.5),
  }
}

const settle = () => new Promise((done) => setTimeout(done, 0))

test('on-device words go after the draft, and finish waits for the last of them', () => {
  const s = setup()
  s.dictation.start(s.box())
  assert.equal(s.dictation.listening, true)
  s.say({ type: 'partial', text: 'in the' })
  assert.equal(s.box(), 'Fix the bug in the')
  s.say({ type: 'final', text: 'in the parser' })
  let sent = false
  s.dictation.finish(() => (sent = true))
  assert.equal(sent, false)
  s.say({ type: 'final', text: 'today' })
  s.say({ type: 'stopped' })
  assert.equal(s.dictation.listening, false)
  assert.equal(s.box(), 'Fix the bug in the parser today')
  assert.equal(sent, true)
  assert.equal(s.dictation.busy, false)
})

test('cancel drops words still on their way', () => {
  const s = setup()
  s.dictation.start(s.box())
  s.dictation.cancel()
  s.say({ type: 'final', text: 'late' })
  assert.equal(s.box(), 'Fix the bug')
  assert.deepEqual(s.log, ['speech.start', 'speech.stop'])
})

test('a speech error ends it and reports it; a send already asked for goes with the words so far', () => {
  const s = setup()
  s.dictation.start(s.box())
  s.say({ type: 'final', text: 'in the parser' })
  let sent = false
  s.dictation.finish(() => (sent = true))
  s.say({ type: 'error', message: 'Speech recognition is off.' })
  assert.equal(s.dictation.error, 'Speech recognition is off.')
  assert.ok(s.log.includes('report Speech recognition is off.'))
  assert.equal(s.dictation.busy, false)
  assert.equal(sent, true)
  assert.equal(s.box(), 'Fix the bug in the parser')
})

test('without on-device speech or a provider, the mic says what to pick', () => {
  const s = setup({ speech: null })
  s.dictation.start(s.box())
  assert.equal(s.dictation.error, 'No speech here. Pick OpenAI or Gemini beside the mic.')
  assert.equal(s.dictation.listening, false)
})

test('a provider without a key opens settings without an error or microphone', async () => {
  const s = setup({ provider: () => 'openai', checkKey: async () => false })
  s.dictation.start(s.box())
  await settle()
  assert.equal(s.dictation.error, null)
  assert.deepEqual(s.log, ['configure openai'])
  assert.ok(!s.log.includes('mic.open'))
  assert.equal(s.dictation.busy, false)
})

test('with a provider it shows listening only once the microphone is open', async () => {
  let open: (stop: () => void) => void = () => {}
  const s = setup({ provider: () => 'openai', microphone: () => new Promise((done) => (open = done)) })
  s.dictation.start(s.box())
  await settle()
  assert.equal(s.dictation.listening, false)
  assert.equal(s.dictation.startedAt, 0)
  assert.equal(s.dictation.busy, true)
  open(() => {})
  await settle()
  assert.equal(s.dictation.listening, true)
  assert.ok(s.dictation.startedAt > 0)
})

test('stopped or cancelled while the microphone opens, it lets the microphone go and never listens', async () => {
  for (const end of ['finish', 'cancel'] as const) {
    let open: (stop: () => void) => void = () => {}
    let closed = false
    const s = setup({ provider: () => 'openai', microphone: () => new Promise((done) => (open = done)) })
    s.dictation.start(s.box())
    await settle()
    s.dictation[end]()
    open(() => (closed = true))
    await settle()
    assert.equal(closed, true, end)
    assert.equal(s.dictation.listening, false, end)
    assert.equal(s.dictation.busy, false, end)
    assert.equal(s.pending.length, 0, end)
  }
})

test("a provider's transcript lands after what's in the box by then, in order, then done runs", async () => {
  const s = setup({ provider: () => 'gemini' })
  s.dictation.start(s.box())
  await settle()
  s.hear(PIECE_SAMPLES)
  s.hear(PIECE_SAMPLES / 2)
  s.type(' in the parser')
  let sent = false
  s.dictation.finish(() => (sent = true))
  assert.ok(s.log.includes('mic.close'))
  assert.equal(s.dictation.transcribing, true)
  await settle()
  assert.equal(s.pending.length, 2)
  s.pending[1]!.answer('second')
  s.pending[0]!.answer('first')
  await settle()
  assert.equal(s.box(), 'Fix the bug in the parser first second')
  assert.ok(s.log.includes(`count ${(PIECE_SAMPLES * 1.5) / 16000}`))
  assert.equal(sent, true)
  assert.equal(s.dictation.transcribing, false)
})

test('cancel while transcribing drops the transcript', async () => {
  const s = setup({ provider: () => 'openai' })
  s.dictation.start(s.box())
  await settle()
  s.hear(MIN_SAMPLES * 2)
  s.dictation.finish()
  await settle()
  s.dictation.cancel()
  s.pending[0]!.answer('too late')
  await settle()
  assert.equal(s.box(), 'Fix the bug')
  assert.equal(s.dictation.busy, false)
})

test('a recording under half a second is dropped without a request', async () => {
  const s = setup({ provider: () => 'openai' })
  s.dictation.start(s.box())
  await settle()
  s.hear(MIN_SAMPLES - 1)
  let sent = false
  s.dictation.finish(() => (sent = true))
  await settle()
  assert.equal(s.pending.length, 0)
  assert.equal(sent, true)
  assert.equal(s.box(), 'Fix the bug')
})

test('a failed transcription reports it and sends nothing', async () => {
  const s = setup({ provider: () => 'openai' })
  s.dictation.start(s.box())
  await settle()
  s.hear(MIN_SAMPLES * 2)
  let sent = false
  s.dictation.finish(() => (sent = true))
  await settle()
  s.pending[0]!.fail(new Error('Host error 500'))
  await settle()
  assert.equal(s.dictation.error, 'Host error 500')
  assert.equal(sent, false)
  assert.equal(s.dictation.busy, false)
})
