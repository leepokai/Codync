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

test("many small messages wait out the relay's 200-message window", () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  const written: string[] = []
  const pacer = new RelayPacer((text) => written.push(text))
  for (let i = 0; i < 250; i++) pacer.send(() => ['{"t":"ping"}'])
  assert.equal(written.length, 200)
  mock.timers.tick(11_000)
  assert.equal(written.length, 250)
  mock.timers.reset()
})

test('a message is made only once the window has room for it, so it can still be given up on while it waits', () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  const written: string[] = []
  const pacer = new RelayPacer((text) => written.push(text))
  for (let i = 0; i < 200; i++) pacer.send(() => ['old'])
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
  assert.deepEqual(written.slice(200), ['next'])
  mock.timers.reset()
})

test('dropping what waits keeps what already left counting against the window', () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  const written: string[] = []
  const pacer = new RelayPacer((text) => written.push(text))
  for (let i = 0; i < 201; i++) pacer.send(() => ['old'])
  pacer.clear()
  pacer.send(() => ['new'])
  assert.equal(written.length, 200)
  mock.timers.tick(11_000)
  assert.deepEqual(written.slice(200), ['new'])
  mock.timers.reset()
})
