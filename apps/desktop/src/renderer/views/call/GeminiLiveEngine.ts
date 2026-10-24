import type { CallLink, CallSession } from './call-link'
import { errorMessage, fromBase64, micPCM16k, toBase64, VoiceEngine } from './voice'

const URL_BASE = 'wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContentConstrained'

/** Gemini's schema spells types in capitals (`OBJECT`, `STRING`). */
function geminiSchema(schema: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(schema)) {
    if (key === 'type' && typeof value === 'string') out[key] = value.toUpperCase()
    else if (key === 'properties' && value && typeof value === 'object')
      out[key] = Object.fromEntries(Object.entries(value).map(([k, v]) => [k, geminiSchema(v as Record<string, unknown>)]))
    else out[key] = value
  }
  return out
}

/**
 * Gemini Live over a WebSocket, this computer ↔ Google directly: 16 kHz PCM up, 24 kHz PCM down,
 * echo-cancelled so you can talk over it. The computer runs the call (`CallLink`): it mints one-use ephemeral tokens
 * on the user's key and writes the voice's instructions and tools, again for each session; a session the server ends
 * (`goAway`) is resumed with a fresh one.
 */
export class GeminiLiveEngine extends VoiceEngine {
  readonly provider = 'gemini' as const
  private socket: WebSocket | null = null
  /** Setup is done on the current socket. */
  private ready = false
  private resumeHandle: string | null = null
  private reconnects = 0
  private stopMic: (() => void) | null = null
  private output: AudioContext | null = null
  private sources = new Set<AudioBufferSourceNode>()
  private nextStart = 0
  /** Buffers scheduled and not played yet; bumping the generation drops late callbacks. */
  private queued = 0
  private playGeneration = 0
  /** The model finished its turn (nothing more is coming until we or the user speak). */
  private turnDone = true
  /** After a local interrupt, the rest of the turn's audio is dropped. */
  private dropping = false
  private pending: string[] = []
  /** The current provider session's instructions and tools, from the computer. */
  private session: CallSession | null = null
  /** Tool calls still running on the computer: replies wait, so the model gets each answer first. */
  private toolsRunning = 0

  constructor(
    private model: string,
    private voice: string,
    private link: CallLink,
  ) {
    super()
  }

  async start() {
    await this.connect()
  }

  private async connect() {
    try {
      const session = await this.link.start('realtime', 'gemini', this.model, this.voice)
      if (this.ended) return
      if (!session.credential) throw new Error('Gemini sent no session credential.')
      this.session = session
      const token = session.credential
      const socket = new WebSocket(`${URL_BASE}?access_token=${encodeURIComponent(token)}`)
      socket.binaryType = 'arraybuffer'
      this.socket = socket
      this.ready = false
      socket.onopen = () => this.sendSetup(socket)
      socket.onmessage = (e) => {
        if (this.socket !== socket) return
        this.received(typeof e.data === 'string' ? e.data : new TextDecoder().decode(e.data as ArrayBuffer))
      }
      socket.onclose = (e) => this.closed(socket, e.reason)
    } catch (e) {
      if (!this.ended) this.fail(errorMessage(e))
    }
  }

  private sendSetup(socket: WebSocket) {
    const tools = (this.session?.tools ?? []).map((t) => ({
      name: t.name,
      description: t.description,
      ...(Object.keys((t.parameters?.properties as object | undefined) ?? {}).length ? { parameters: geminiSchema(t.parameters!) } : {}),
    }))
    const setup = {
      model: `models/${this.model}`,
      generationConfig: {
        responseModalities: ['AUDIO'],
        speechConfig: { voiceConfig: { prebuiltVoiceConfig: { voiceName: this.voice } } },
      },
      systemInstruction: { parts: [{ text: this.session?.instructions ?? '' }] },
      tools: [{ functionDeclarations: tools }],
      // Audio sessions are otherwise capped at 15 minutes.
      contextWindowCompression: { slidingWindow: {} },
      // What both sides said, for the call's line in the chat.
      inputAudioTranscription: {},
      outputAudioTranscription: {},
      sessionResumption: this.resumeHandle ? { handle: this.resumeHandle } : {},
    }
    this.send({ setup }, socket)
  }

  end() {
    this.ended = true
    this.onActivityChanged?.(false)
    const socket = this.socket
    this.socket = null
    socket?.close(1000)
    this.stopPlayback()
    this.stopMic?.()
    this.stopMic = null
    void this.output?.close()
    this.output = null
  }

  protected mutedChanged(on: boolean) {
    this.level = 0
    if (on) this.send({ realtimeInput: { audioStreamEnd: true } })
  }

  interrupt() {
    if (this.phase !== 'speaking') return
    this.stopPlayback()
    this.dropping = !this.turnDone
    this.setPhase('listening')
  }

  speak(reply: string) {
    if (this.ended || this.phase === 'failed') return
    this.pending.push(this.link.reply(reply))
    this.flush()
  }

  announce(notice: string) {
    if (this.ended || this.phase === 'failed') return
    this.pending.push(this.link.notice(notice))
    this.flush()
  }

  // MARK: socket

  private closed(socket: WebSocket, reason: string) {
    if (this.ended || this.socket !== socket) return
    this.socket = null
    this.ready = false
    if (this.resumeHandle && this.reconnects < 3) {
      this.reconnects++
      void this.connect()
    } else {
      this.fail(reason || 'Lost the connection to Gemini.')
    }
  }

  private received(data: string) {
    if (this.ended) return
    let msg: Record<string, any>
    try {
      msg = JSON.parse(data) as Record<string, any>
    } catch {
      return
    }
    if (msg.setupComplete !== undefined) {
      this.ready = true
      this.reconnects = 0
      if (!this.stopMic) void this.startAudio()
      this.flush()
    }
    const content = msg.serverContent
    if (content) {
      if (typeof content.inputTranscription?.text === 'string') this.link.add('user', content.inputTranscription.text)
      if (typeof content.outputTranscription?.text === 'string') this.link.add('voice', content.outputTranscription.text)
      if (content.turnComplete === true || content.interrupted === true) this.link.endTurn()
      if (content.interrupted === true) {
        this.stopPlayback()
        this.dropping = false
        if (this.phase === 'speaking') this.setPhase('listening')
      }
      const parts = content.modelTurn?.parts
      if (Array.isArray(parts)) {
        this.turnDone = false
        for (const part of parts) {
          const audio = part?.inlineData?.data
          if (typeof audio === 'string' && !this.dropping) this.play(audio)
        }
      }
      if (content.turnComplete === true) {
        this.turnDone = true
        this.dropping = false
        if (this.queued === 0 && this.phase === 'speaking') this.setPhase('listening')
        this.flush()
      }
    }
    const calls = msg.toolCall?.functionCalls
    if (Array.isArray(calls)) {
      // The tools run on the computer; their answers go back together.
      const asked = calls.filter((c) => typeof c?.id === 'string' && typeof c?.name === 'string')
      this.toolsRunning++
      void Promise.all(asked.map(async (c) => ({ id: c.id, name: c.name, response: { result: await this.link.tool(c.name, c.args ?? {}) } })))
        .then((functionResponses) => {
          this.toolsRunning--
          this.send({ toolResponse: { functionResponses } })
          this.flush()
        })
    }
    const update = msg.sessionResumptionUpdate
    if (update?.resumable === true && typeof update.newHandle === 'string') this.resumeHandle = update.newHandle
    // The server ends this connection soon; carry on in a resumed one.
    if (msg.goAway !== undefined) this.socket?.close(1000)
  }

  private send(message: unknown, socket?: WebSocket) {
    const target = socket ?? (this.ready ? this.socket : null)
    if (target?.readyState === WebSocket.OPEN) target.send(JSON.stringify(message))
  }

  /** Replies wait for the model to finish talking; `turnComplete` here would cut it off. */
  private flush() {
    if (!this.ready || !this.turnDone || this.queued !== 0 || this.toolsRunning > 0 || !this.pending.length) return
    const text = this.pending.join('\n\n')
    this.pending = []
    this.turnDone = false
    this.send({ clientContent: { turns: [{ role: 'user', parts: [{ text }] }], turnComplete: true } })
  }

  // MARK: audio

  private async startAudio() {
    try {
      this.output = new AudioContext({ sampleRate: 24000 })
      const stop = await micPCM16k((pcm, level) => this.captured(pcm, level))
      if (this.ended) return stop()
      this.stopMic = stop
    } catch (e) {
      if (!this.ended) this.fail(errorMessage(e))
    }
  }

  private captured(pcm: Int16Array, level: number) {
    // Connected once the microphone delivers after setup, not when the socket opens.
    if (!this.ended && this.ready && this.phase === 'starting') this.setPhase('listening')
    if (this.ended || this.muted || !this.isActive) return
    this.level = this.phase === 'speaking' ? 0 : level
    this.send({ realtimeInput: { audio: { data: toBase64(new Uint8Array(pcm.buffer)), mimeType: 'audio/pcm;rate=16000' } } })
  }

  private play(base64: string) {
    const ctx = this.output
    if (!ctx) return
    const bytes = fromBase64(base64)
    const frames = bytes.length >> 1
    if (!frames) return
    const view = new DataView(bytes.buffer)
    const buffer = ctx.createBuffer(1, frames, 24000)
    const out = buffer.getChannelData(0)
    for (let i = 0; i < frames; i++) out[i] = view.getInt16(i * 2, true) / 32768
    const source = ctx.createBufferSource()
    source.buffer = buffer
    source.connect(ctx.destination)
    this.nextStart = Math.max(this.nextStart, ctx.currentTime)
    source.start(this.nextStart)
    this.nextStart += buffer.duration
    this.sources.add(source)
    this.queued++
    this.level = 0
    if (this.phase !== 'speaking') this.setPhase('speaking')
    const generation = this.playGeneration
    source.onended = () => {
      this.sources.delete(source)
      this.played(generation)
    }
  }

  private played(generation: number) {
    if (generation !== this.playGeneration || this.ended) return
    this.queued = Math.max(0, this.queued - 1)
    if (this.queued !== 0 || !this.turnDone) return
    if (this.phase === 'speaking') this.setPhase('listening')
    this.flush()
  }

  private stopPlayback() {
    this.playGeneration++
    this.queued = 0
    this.nextStart = 0
    for (const s of this.sources) {
      s.onended = null
      try {
        s.stop()
      } catch {}
    }
    this.sources.clear()
  }
}
