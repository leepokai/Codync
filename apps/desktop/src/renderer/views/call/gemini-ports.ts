import { micPCM16k } from './voice'

// What Gemini Live needs from the computer (GeminiLiveEngine): its WebSocket, the microphone and the speaker.

/** The WebSocket the engine uses (a browser `WebSocket`; tests replay a recorded one). */
export interface LiveSocket {
  readonly readyState: number
  onopen: (() => void) | null
  onmessage: ((e: { data: unknown }) => void) | null
  onclose: ((e: { code: number; reason: string }) => void) | null
  send(data: string): void
  close(code?: number): void
}

/** The voice's audio out: 24 kHz PCM16 pieces, `done` once each has played. */
export interface CallOutput {
  play(pcm: Uint8Array, done: () => void): void
  /** Drops what's queued. */
  stop(): void
  close(): void
}

/** What the engine needs from the computer; tests script it. */
export interface GeminiPorts {
  socket(url: string): LiveSocket
  mic(out: (pcm: Int16Array, level: number) => void): Promise<() => void>
  output(): CallOutput
}

/** Web Audio playback, each piece scheduled after the last. */
class WebAudioOutput implements CallOutput {
  private ctx = new AudioContext({ sampleRate: 24000 })
  private sources = new Set<AudioBufferSourceNode>()
  private nextStart = 0

  play(bytes: Uint8Array, done: () => void) {
    const frames = bytes.length >> 1
    if (!frames) return
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    const buffer = this.ctx.createBuffer(1, frames, 24000)
    const out = buffer.getChannelData(0)
    for (let i = 0; i < frames; i++) out[i] = view.getInt16(i * 2, true) / 32768
    const source = this.ctx.createBufferSource()
    source.buffer = buffer
    source.connect(this.ctx.destination)
    this.nextStart = Math.max(this.nextStart, this.ctx.currentTime)
    source.start(this.nextStart)
    this.nextStart += buffer.duration
    this.sources.add(source)
    source.onended = () => {
      this.sources.delete(source)
      done()
    }
  }

  stop() {
    this.nextStart = 0
    for (const s of this.sources) {
      s.onended = null
      try {
        s.stop()
      } catch {}
    }
    this.sources.clear()
  }

  close() {
    this.stop()
    void this.ctx.close()
  }
}

export const browserPorts: GeminiPorts = {
  socket: (url) => {
    const socket = new WebSocket(url)
    socket.binaryType = 'arraybuffer'
    return socket as unknown as LiveSocket
  },
  mic: micPCM16k,
  output: () => new WebAudioOutput(),
}
