import { createHash } from 'node:crypto'
import { createServer, type IncomingMessage } from 'node:http'
import { connect, type Socket } from 'node:net'

const GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11'

export interface VncBridge {
  /** WebSocket URL noVNC connects to. It speaks RFB to `127.0.0.1:vncPort`. */
  url: string
  close: () => void
}

/**
 * A loopback WebSocket that copies bytes to a loopback VNC port.
 * noVNC cannot open a TCP socket, and the VNC server is reached only through
 * the SSH forward. Nothing here listens outside 127.0.0.1.
 */
export function bridgeVnc(vncPort: number): Promise<VncBridge> {
  const server = createServer((_req, res) => {
    res.writeHead(426).end()
  })
  const sockets = new Set<Socket>()
  return new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      if (!address || typeof address === 'string') {
        reject(new Error("Couldn't open the screen bridge."))
        return
      }
      server.on('upgrade', (req: IncomingMessage, socket, head) => {
        const key = req.headers['sec-websocket-key']
        if (req.headers.upgrade?.toLowerCase() !== 'websocket' || typeof key !== 'string') {
          socket.destroy()
          return
        }
        const accept = createHash('sha1').update(key + GUID).digest('base64')
        socket.write(
          `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`,
        )
        const tcp = connect(vncPort, '127.0.0.1')
        sockets.add(tcp)
        let pending = Buffer.alloc(0) as Buffer
        let incoming = Buffer.alloc(0) as Buffer
        let open = false
        const close = () => {
          tcp.destroy()
          socket.destroy()
        }
        const take = (payload: Buffer) => {
          if (open) tcp.write(payload)
          else pending = Buffer.concat([pending, payload]) as Buffer
        }
        socket.on('data', (data) => {
          incoming = Buffer.concat([incoming, Buffer.isBuffer(data) ? data : Buffer.from(data)]) as Buffer
          const read = readFrames(incoming)
          incoming = Buffer.from(read.rest)
          for (const frame of read.frames) {
            if (frame.opcode === 0x8) {
              close()
              return
            }
            if (frame.opcode === 0x2 || frame.opcode === 0x1) take(frame.payload)
          }
        })
        tcp.on('connect', () => {
          open = true
          if (pending.length) tcp.write(pending)
          pending = Buffer.alloc(0)
        })
        tcp.on('data', (chunk) => {
          if (!socket.destroyed) socket.write(encodeFrame(chunk))
        })
        tcp.on('error', close)
        tcp.on('close', () => socket.destroy())
        socket.on('error', () => tcp.destroy())
        socket.on('close', () => {
          sockets.delete(tcp)
          tcp.destroy()
        })
        if (head.length) socket.unshift(head)
      })
      resolve({
        url: `ws://127.0.0.1:${address.port}/websockify`,
        close: () => {
          for (const tcp of sockets) tcp.destroy()
          server.close()
        },
      })
    })
  })
}

interface Frame {
  opcode: number
  payload: Buffer
}

/** Complete frames, plus any trailing bytes that are not a whole frame yet. */
export function readFrames(data: Buffer): { frames: Frame[]; rest: Buffer } {
  const frames: Frame[] = []
  let offset = 0
  while (offset + 2 <= data.length) {
    const opcode = data[offset]! & 0x0f
    const masked = (data[offset + 1]! & 0x80) !== 0
    let length = data[offset + 1]! & 0x7f
    let start = offset + 2
    if (length === 126) {
      if (start + 2 > data.length) break
      length = data.readUInt16BE(start)
      start += 2
    } else if (length === 127) {
      if (start + 8 > data.length) break
      length = data.readUInt32BE(start) * 0x100000000 + data.readUInt32BE(start + 4)
      start += 8
    }
    const maskBytes = masked ? 4 : 0
    if (start + maskBytes + length > data.length) break
    let payload: Buffer
    if (masked) {
      const mask = data.subarray(start, start + 4)
      const raw = data.subarray(start + 4, start + 4 + length)
      payload = Buffer.alloc(length)
      for (let i = 0; i < raw.length; i++) payload[i] = raw[i]! ^ mask[i % 4]!
    } else {
      payload = Buffer.from(data.subarray(start, start + length))
    }
    frames.push({ opcode, payload })
    offset = start + maskBytes + length
  }
  return { frames, rest: data.subarray(offset) }
}

/** A binary server frame. The bridge is the server, so the payload is not masked. */
export function encodeFrame(payload: Buffer): Buffer {
  const header = payload.length < 126 ? Buffer.alloc(2) : payload.length < 65536 ? Buffer.alloc(4) : Buffer.alloc(10)
  header[0] = 0x82
  if (payload.length < 126) {
    header[1] = payload.length
  } else if (payload.length < 65536) {
    header[1] = 126
    header.writeUInt16BE(payload.length, 2)
  } else {
    header[1] = 127
    header.writeUInt32BE(Math.floor(payload.length / 0x100000000), 2)
    header.writeUInt32BE(payload.length >>> 0, 6)
  }
  return Buffer.concat([header, payload])
}
