import assert from 'node:assert/strict'
import { createServer } from 'node:net'
import { test } from 'node:test'
import { bridgeVnc, encodeFrame, readFrames } from './vnc-bridge.ts'

test('websocket frames round-trip a masked client payload', () => {
  const payload = Buffer.from('RFB 003.008\n')
  const mask = Buffer.from([1, 2, 3, 4])
  const masked = Buffer.alloc(payload.length)
  for (let i = 0; i < payload.length; i++) masked[i] = payload[i]! ^ mask[i % 4]!
  const frame = Buffer.concat([Buffer.from([0x82, 0x80 | payload.length]), mask, masked])
  const read = readFrames(frame)
  assert.equal(read.frames.length, 1)
  assert.deepEqual(read.frames[0]?.payload, payload)
  assert.equal(read.rest.length, 0)
  const again = readFrames(encodeFrame(payload))
  assert.deepEqual(again.frames[0]?.payload, payload)
})

test('the bridge copies bytes between the websocket and the VNC port', async () => {
  const tcp = createServer((socket) => {
    socket.on('data', (chunk) => socket.write(Buffer.concat([Buffer.from('out:'), chunk])))
  })
  await new Promise<void>((resolve) => tcp.listen(0, '127.0.0.1', resolve))
  const address = tcp.address()
  assert.ok(address && typeof address === 'object')
  const bridge = await bridgeVnc(address.port)
  try {
    const ws = new WebSocket(bridge.url)
    ws.binaryType = 'arraybuffer'
    const received = new Promise<Buffer>((resolve, reject) => {
      ws.addEventListener('message', (event) => {
        const data = event.data
        if (data instanceof ArrayBuffer) resolve(Buffer.from(data))
        else reject(new Error('expected binary'))
      })
      ws.addEventListener('error', () => reject(new Error('websocket failed')))
    })
    await new Promise<void>((resolve, reject) => {
      ws.addEventListener('open', () => resolve())
      ws.addEventListener('error', () => reject(new Error('websocket failed')))
    })
    ws.send(Buffer.from('click'))
    assert.equal((await received).toString(), 'out:click')
    ws.close()
  } finally {
    bridge.close()
    await new Promise<void>((resolve) => tcp.close(() => resolve()))
  }
})
