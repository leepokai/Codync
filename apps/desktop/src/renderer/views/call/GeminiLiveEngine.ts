import type { CallLink, CallSession } from './call-link'
import { browserPorts, type CallOutput, type GeminiPorts, type LiveSocket } from './gemini-ports'
import { errorMessage, fromBase64, onDevicesChanged, PROGRESS_TAG, reconnect, toBase64, VoiceEngine } from './voice'

const URL_BASE = 'wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContentConstrained'
const OPEN = 1

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
 * (`goAway`) is resumed with a fresh one, and a dropped one is tried again.
 */
export class GeminiLiveEngine extends VoiceEngine {
  /** Without playback progress this long (ms) after the voice's turn, the call stops waiting for the speaker. */
  static stallMs = 4000
  readonly provider = 'gemini' as const
  /** Playback stopped reporting progress after the voice's turn, and the call went back to listening. */
  onStall: (() => void) | null = null
  private socket: LiveSocket | null = null
  /** Setup is done on the current socket. */
  private ready = false
  /** A session was set up once: a drop after that is tried again instead of ending the call. */
  private connected = false
  /** The server said it ends this connection (`goAway`): carry on in a resumed one at once. */
  private replacing = false
  private resumeHandle: string | null = null
  private reconnects = 0
  private stopMic: (() => void) | null = null
  private stopDevices: (() => void) | null = null
  private output: CallOutput | null = null
  /** Pieces handed to the speaker and not played yet; bumping the generation drops late callbacks. */
  private queued = 0
  private playGeneration = 0
  private lastPlayed = 0
  private watchdog: ReturnType<typeof setInterval> | null = null
  /** The model finished its turn (nothing more is coming until we or the user speak). */
  private turnDone = true
  /** After a local interrupt, the rest of the turn's audio is dropped. */
  private dropping = false
  private pending: string[] = []
  /** The current provider session's instructions and tools, from the computer. */
  private session: CallSession | null = null
  /** Tool calls still running on the computer: replies wait, so the model gets each answer first. */
  private toolsRunning = 0

  private model: string
  private voice: string
  private link: CallLink
  private ports: GeminiPorts

  constructor(model: string, voice: string, link: CallLink, ports: GeminiPorts = browserPorts) {
    super()
    this.model = model
    this.voice = voice
    this.link = link
    this.ports = ports
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
      const socket = this.ports.socket(`${URL_BASE}?access_token=${encodeURIComponent(session.credential)}`)
      this.socket = socket
      this.ready = false
      socket.onopen = () => this.sendSetup(socket)
      socket.onmessage = (e) => {
        if (this.socket !== socket) return
        this.received(typeof e.data === 'string' ? e.data : new TextDecoder().decode(e.data as ArrayBuffer))
      }
      socket.onclose = (e) => this.closed(socket, e.code, e.reason)
    } catch (e) {
      if (this.ended) return
      // Starting fails over; mid-call, the computer or Google may be back in a moment.
      if (this.connected) this.dropped(errorMessage(e))
      else this.fail(errorMessage(e), 'provider')
    }
  }

  private sendSetup(socket: LiveSocket) {
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
    this.stopDevices?.()
    this.stopDevices = null
    this.stopMic?.()
    this.stopMic = null
    this.output?.close()
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

  /** Only the newest progress waits to be said. */
  progress(text: string) {
    if (this.ended || this.phase === 'failed') return
    this.pending = this.pending.filter((p) => !p.startsWith(PROGRESS_TAG))
    this.pending.push(this.link.progress(text))
    this.flush()
  }

  // MARK: socket

  private closed(socket: LiveSocket, code: number, reason: string) {
    if (this.ended || this.socket !== socket) return
    this.socket = null
    this.ready = false
    if (!this.connected) return this.fail(reason || "Couldn't reach Gemini.", 'provider')
    // Gemini refused the session (a key, a model, a request): trying again won't help.
    if (code === 1007 || code === 1008) return this.fail(reason || 'Gemini ended the call.', 'provider')
    if (this.replacing) {
      this.replacing = false
      // The turn under way ends with this connection: the resumed one won't complete it.
      this.turnEnded()
      void this.connect()
    } else {
      this.dropped(reason)
    }
  }

  /**
   * The connection to Gemini dropped mid-call: tried again a few times (a resumed session when Gemini gave a handle,
   * else a fresh one the computer primes with the call so far) before the call falls back.
   */
  private dropped(reason: string) {
    if (this.reconnects >= reconnect.delays.length) return this.fail(reason || 'Lost the connection to Gemini.', 'connection')
    const delay = reconnect.delays[this.reconnects]!
    this.reconnects++
    this.stopPlayback()
    this.turnDone = true
    this.dropping = false
    this.setPhase('reconnecting')
    setTimeout(() => {
      if (!this.ended) void this.connect()
    }, delay)
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
      this.connected = true
      this.reconnects = 0
      if (!this.output) void this.startAudio()
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
      if (content.turnComplete === true) this.turnEnded()
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
    if (msg.goAway !== undefined) {
      // The server ends this connection soon; carry on in a resumed one.
      this.replacing = true
      this.socket?.close(1000)
    }
  }

  /** The model's turn is over: listening once what's queued has played. */
  private turnEnded() {
    this.turnDone = true
    this.dropping = false
    if (this.queued === 0) this.finishSpeaking()
    else this.watch()
  }

  private send(message: unknown, socket?: LiveSocket) {
    const target = socket ?? (this.ready ? this.socket : null)
    if (target?.readyState === OPEN) target.send(JSON.stringify(message))
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
    this.output = this.ports.output()
    await this.openMic()
    // A headset plugged in or out: the microphone starts again on what the system picks now.
    if (!this.ended && this.stopMic) this.stopDevices ??= onDevicesChanged(() => void this.restartMic())
  }

  private async openMic() {
    try {
      const stop = await this.ports.mic((pcm, level) => this.captured(pcm, level))
      if (this.ended) return stop()
      this.stopMic = stop
    } catch (e) {
      if (!this.ended) this.fail(errorMessage(e), 'microphone')
    }
  }

  private async restartMic() {
    if (this.ended || !this.stopMic) return
    this.stopMic()
    this.stopMic = null
    if (this.isActive) this.setPhase('reconnecting')
    await this.openMic()
  }

  private captured(pcm: Int16Array, level: number) {
    // Ready once the microphone delivers after setup (again, after a reconnect), not when the socket opens.
    if (!this.ended && this.ready && this.isConnecting) {
      this.setPhase('listening')
      this.flush()
    }
    if (this.ended || this.muted || !this.isActive) return
    this.level = this.phase === 'speaking' ? 0 : level
    this.send({ realtimeInput: { audio: { data: toBase64(new Uint8Array(pcm.buffer)), mimeType: 'audio/pcm;rate=16000' } } })
  }

  private play(base64: string) {
    const output = this.output
    if (!output) return
    const bytes = fromBase64(base64)
    if (bytes.length < 2) return
    this.queued++
    this.level = 0
    if (this.phase !== 'speaking') this.setPhase('speaking')
    const generation = this.playGeneration
    output.play(bytes, () => this.played(generation))
  }

  private played(generation: number) {
    if (generation !== this.playGeneration || this.ended) return
    this.lastPlayed = Date.now()
    this.queued = Math.max(0, this.queued - 1)
    if (this.queued !== 0 || !this.turnDone) return
    this.finishSpeaking()
  }

  private finishSpeaking() {
    this.stopWatch()
    if (this.phase === 'speaking') this.setPhase('listening')
    this.flush()
  }

  /** After the voice's turn, playback that stops reporting progress doesn't hold the call (see `stallMs`). */
  private watch() {
    this.stopWatch()
    this.lastPlayed = Date.now()
    this.watchdog = setInterval(() => {
      if (this.ended || this.queued === 0 || !this.turnDone) return this.stopWatch()
      if (Date.now() - this.lastPlayed < GeminiLiveEngine.stallMs) return
      console.error('gemini playback stalled; back to listening')
      this.playGeneration++
      this.queued = 0
      this.onStall?.()
      this.finishSpeaking()
    }, GeminiLiveEngine.stallMs / 2)
  }

  private stopWatch() {
    if (this.watchdog) clearInterval(this.watchdog)
    this.watchdog = null
  }

  private stopPlayback() {
    this.stopWatch()
    this.playGeneration++
    this.queued = 0
    this.output?.stop()
  }
}
