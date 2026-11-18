import assert from 'node:assert/strict'
import { mock, test } from 'node:test'
import { RelayPacer } from './relay-pacer.ts'

test("a large upload waits out the relay's 12 MiB window instead of overrunning it, in order", () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  const written: string[] = []
  const pacer = new RelayPacer((text) => written.push(text))
  const frames = Array.from({ length: 20 }, (_, i) => String(i).padEnd(700 * 1024))
  pacer.send(() => [...frames])
  assert.equal(written.length, 17)
  mock.timers.tick(10_999)
  assert.equal(written.length, 17)
  mock.timers.tick(1)
  assert.deepEqual(written, frames)
  mock.timers.reset()
})

test("many small messages wait out the relay's 200-message window, short of the room kept for handshakes", () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  const written: string[] = []
  const pacer = new RelayPacer((text) => written.push(text))
  for (let i = 0; i < 250; i++) pacer.send(() => ['{"t":"ping"}'])
  assert.equal(written.length, 192)
  mock.timers.tick(11_000)
  assert.equal(written.length, 250)
  mock.timers.reset()
})

test("a handshake leaves at once while paced messages wait out the relay's window, and once the window is full, as soon as a slot frees", () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  const written: string[] = []
  const pacer = new RelayPacer((text) => written.push(text))
  pacer.send(() => ['early'])
  mock.timers.tick(1_000)
  for (let i = 0; i < 191; i++) pacer.send(() => ['full'])
  for (let i = 0; i < 9; i++) pacer.sendFirst(`hello ${i}`)
  pacer.send(() => ['late'])
  assert.equal(written.length, 200)
  assert.deepEqual(written.slice(192), Array.from({ length: 8 }, (_, i) => `hello ${i}`))
  // The first message's slot frees at 11 s: the last handshake takes it, the paced one waits for more room.
  mock.timers.tick(10_000)
  assert.deepEqual(written.slice(200), ['hello 8'])
  mock.timers.tick(1_000)
  assert.deepEqual(written.slice(201), ['late'])
  mock.timers.reset()
})

test('a message is made only once the window has room for it, so it can still be given up on while it waits', () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  const written: string[] = []
  const pacer = new RelayPacer((text) => written.push(text))
  for (let i = 0; i < 192; i++) pacer.send(() => ['old'])
  let madeAt: number | null = null
  let wanted = true
  pacer.send(() => {
    madeAt = Date.now()
    return wanted ? ['given up'] : []
  })
  pacer.send(() => ['next'])
  assert.equal(madeAt, null)
  wanted = false
  mock.timers.tick(11_000)
  assert.equal(madeAt, 11_000)
  assert.deepEqual(written.slice(192), ['next'])
  mock.timers.reset()
})

test('dropping what waits keeps what already left counting against the window', () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  const written: string[] = []
  const pacer = new RelayPacer((text) => written.push(text))
  for (let i = 0; i < 193; i++) pacer.send(() => ['old'])
  pacer.clear()
  pacer.send(() => ['new'])
  assert.equal(written.length, 192)
  mock.timers.tick(11_000)
  assert.deepEqual(written.slice(192), ['new'])
  mock.timers.reset()
})
