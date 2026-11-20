import assert from 'node:assert/strict'
import test from 'node:test'
import { latestOnly } from './latest-only.ts'

/** A send whose calls finish when the test says. */
function controlled() {
  const sent: number[] = []
  const finish: ((failure?: Error) => void)[] = []
  const send = (value: number) => {
    sent.push(value)
    return new Promise<void>((resolve, reject) => finish.push((failure) => (failure ? reject(failure) : resolve())))
  }
  return { sent, finish, send }
}

const turn = () => new Promise((resolve) => setImmediate(resolve))

test('sends one at a time and skips to the latest', async () => {
  const { sent, finish, send } = controlled()
  const order = latestOnly(send)
  const all = [order(1), order(2), order(3)]
  assert.deepEqual(sent, [1])
  finish.shift()?.()
  await turn()
  assert.deepEqual(sent, [1, 3], 'the one in between never goes out')
  finish.shift()?.()
  await Promise.all(all)
})

test('starts again after it finished', async () => {
  const { sent, finish, send } = controlled()
  const order = latestOnly(send)
  const first = order(1)
  finish.shift()?.()
  await first
  const second = order(2)
  assert.deepEqual(sent, [1, 2])
  finish.shift()?.()
  await second
})

test('a failure reaches every caller and drops what was waiting', async () => {
  const { sent, finish, send } = controlled()
  const order = latestOnly(send)
  const all = [order(1), order(2)]
  finish.shift()?.(new Error('refused'))
  for (const call of all) await assert.rejects(call, /refused/)
  assert.deepEqual(sent, [1])
  const next = order(3)
  assert.deepEqual(sent, [1, 3], 'the next call starts again')
  finish.shift()?.()
  await next
})
