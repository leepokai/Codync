import assert from 'node:assert/strict'
import { mock, test } from 'node:test'
import { RelayPacer } from '../main/relay-pacer.ts'

test("a large upload waits out the relay's 12 MiB window instead of overrunning it, in order", () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  const written: string[] = []
  const pacer = new RelayPacer((text) => written.push(text))
  const frames = Array.from({ length: 20 }, (_, i) => String(i).padEnd(700 * 1024))
  for (const frame of frames) pacer.send(frame)
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
  for (let i = 0; i < 250; i++) pacer.send('{"t":"ping"}')
  assert.equal(written.length, 200)
  mock.timers.tick(11_000)
  assert.equal(written.length, 250)
  mock.timers.reset()
})

test('a stopped link sends nothing more', () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] })
  const written: string[] = []
  const pacer = new RelayPacer((text) => written.push(text))
  for (let i = 0; i < 201; i++) pacer.send('x')
  pacer.stop()
  mock.timers.tick(11_000)
  assert.equal(written.length, 200)
  mock.timers.reset()
})
