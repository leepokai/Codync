import type { CallLink, CallSession } from './call-link'
import { errorMessage, microphone, onDevicesChanged, PROGRESS_TAG, reconnect, VoiceEngine, VoiceSettings } from './voice'

/**
 * OpenAI Realtime over WebRTC, this computer ↔ OpenAI directly: the microphone is a WebRTC audio
 * track, the model's voice plays from the remote track, and events (tools, replies) go over
 * `oai-events`. The computer runs the call (`CallLink`): it mints the client secret on the user's key and writes the
 * voice's instructions and tools; it never sees audio.
 */
export class OpenAIRealtimeEngine extends VoiceEngine {
  readonly provider = 'openai' as const
  private pc: RTCPeerConnection | null = null
  private events: RTCDataChannel | null = null
  private stream: MediaStream | null = null
  private audio: HTMLAudioElement | null = null
  /** A response is being generated; new replies wait for it to finish. */
  private responding = false
  private pending: string[] = []
  private dropTimer: ReturnType<typeof setTimeout> | null = null
  private session: CallSession | null = null
  /** Tool calls still running on the computer: replies wait, so the model gets each result first. */
  private toolsRunning = 0
  /** The session was set up once: a drop after that is tried again instead of ending the call. */
  private connected = false
  private reconnects = 0
  private stopDevices: (() => void) | null = null

  private model: string
  private voice: string
  private link: CallLink

  constructor(model: string, voice: string, link: CallLink) {
    super()
    this.model = model
    this.voice = voice
    this.link = link
  }

  async start() {
    try {
      this.stream = await microphone()
    } catch (e) {
      return void (!this.ended && this.fail(errorMessage(e), 'microphone'))
    }
    if (this.ended) return this.release()
    // A headset plugged in or out: the call's microphone track moves to what the system picks now.
    this.stopDevices = onDevicesChanged(() => void this.replaceMic())
    await this.open()
  }

  /**
   * A session with OpenAI: the computer mints its client secret and writes the voice's instructions (with the call so
   * far, for a session that replaces a dropped one).
   */
  private async open() {
    try {
      const session = await this.link.start('realtime', 'openai', this.model, this.voice)
      if (this.ended) return
      if (!session.credential) throw new Error('OpenAI sent no session credential.')
      this.session = session
      await this.connect(session.credential)
    } catch (e) {
      if (this.ended) return
      if (this.connected) this.dropped(errorMessage(e))
      else this.fail(errorMessage(e), 'provider')
    }
  }

  /** The connection to OpenAI dropped mid-call: a new session, a few times, before the call falls back. */
  private dropped(reason: string) {
    if (this.ended) return
    if (this.reconnects >= reconnect.delays.length) return this.fail(reason, 'connection')
    const delay = reconnect.delays[this.reconnects]!
    this.reconnects++
    this.close()
    this.setPhase('reconnecting')
    setTimeout(() => {
      if (!this.ended) void this.open()
    }, delay)
  }

  private async replaceMic() {
    if (this.ended || !this.stream) return
    try {
      const next = await microphone()
      if (this.ended) return void next.getTracks().forEach((t) => t.stop())
      const track = next.getAudioTracks()[0]
      if (!track) return
      track.enabled = !this.muted
      await this.pc?.getSenders().find((s) => s.track?.kind === 'audio')?.replaceTrack(track)
      this.release()
      this.stream = next
    } catch (e) {
      console.error('realtime: microphone change', errorMessage(e))
    }
  }

  private async connect(secret: string) {
    const pc = new RTCPeerConnection({ bundlePolicy: 'max-bundle' })
    this.pc = pc
    const track = this.stream!.getAudioTracks()[0]
    if (!track) throw new Error("Couldn't start the call audio.")
    track.enabled = !this.muted
    pc.addTrack(track, this.stream!)
    const audio = new Audio()
    audio.autoplay = true
    this.audio = audio
    pc.ontrack = (e) => {
      audio.srcObject = e.streams[0] ?? new MediaStream([e.track])
    }
    pc.oniceconnectionstatechange = () => this.pc === pc && this.iceChanged(pc.iceConnectionState)
    const events = pc.createDataChannel('oai-events')
    events.onopen = () => this.opened()
    events.onmessage = (e) => typeof e.data === 'string' && this.received(e.data)
    events.onclose = () => this.events === events && this.connected && this.dropped('Lost the connection to OpenAI.')
    this.events = events

    const offer = await pc.createOffer()
    await pc.setLocalDescription(offer)
    const response = await fetch('https://api.openai.com/v1/realtime/calls', {
      method: 'POST',
      headers: { Authorization: `Bearer ${secret}`, 'Content-Type': 'application/sdp' },
      body: offer.sdp,
      signal: AbortSignal.timeout(15_000),
    })
    const body = await response.text()
    if (this.ended) return
    if (!response.ok) {
      let message: string | undefined
      try {
        message = (JSON.parse(body) as { error?: { message?: string } }).error?.message
      } catch {}
      throw new Error(message ?? `OpenAI answered ${response.status}.`)
    }
    await pc.setRemoteDescription({ type: 'answer', sdp: body })
    this.dropTimer = setTimeout(() => {
      if (this.ended || !this.isConnecting) return
      if (this.connected) this.dropped("Couldn't reach OpenAI.")
      else this.fail("Couldn't reach OpenAI.", 'connection')
    }, 15_000)
  }

  private release() {
    for (const t of this.stream?.getTracks() ?? []) t.stop()
    this.stream = null
  }

  end() {
    this.ended = true
    this.onActivityChanged?.(false)
    this.stopDevices?.()
    this.stopDevices = null
    this.close()
    this.release()
  }

  /** Closes the session, keeping the microphone for the next one. */
  private close() {
    if (this.dropTimer) clearTimeout(this.dropTimer)
    const events = this.events
    this.events = null
    events?.close()
    this.pc?.close()
    this.pc = null
    this.responding = false
    if (this.audio) {
      this.audio.srcObject = null
      this.audio = null
    }
  }

  protected mutedChanged(on: boolean) {
    for (const t of this.stream?.getAudioTracks() ?? []) t.enabled = !on
    if (on) this.level = 0
  }

  interrupt() {
    if (this.phase !== 'speaking') return
    this.send({ type: 'response.cancel' })
    this.send({ type: 'output_audio_buffer.clear' })
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

  // MARK: events

  private opened() {
    if (this.ended) return
    this.send({
      type: 'session.update',
      session: {
        type: 'realtime',
        instructions: this.session?.instructions ?? '',
        tools: (this.session?.tools ?? []).map((t) => ({ type: 'function', name: t.name, description: t.description, parameters: t.parameters ?? { type: 'object', properties: {} } })),
        // What the user says is transcribed too, for the call's line in the chat.
        audio: { input: { turn_detection: { type: 'semantic_vad' }, transcription: { model: VoiceSettings.transcribeModel('openai') } } },
      },
    })
  }

  private received(data: string) {
    if (this.ended) return
    let msg: Record<string, unknown>
    try {
      msg = JSON.parse(data) as Record<string, unknown>
    } catch {
      return
    }
    switch (msg.type) {
      case 'session.updated':
        // Connected once the session has the instructions and tools, not when the channel opens.
        if (!this.isConnecting) break
        if (this.dropTimer) clearTimeout(this.dropTimer)
        this.connected = true
        this.reconnects = 0
        this.setPhase('listening')
        this.flush()
        break
      case 'input_audio_buffer.speech_started':
        this.level = this.muted ? 0 : 0.7
        break
      case 'input_audio_buffer.speech_stopped':
        this.level = 0
        break
      case 'input_audio_buffer.committed':
        this.link.transcribing(true)
        break
      case 'conversation.item.input_audio_transcription.failed':
        this.link.transcribing(false)
        break
      case 'output_audio_buffer.started':
        if (this.isActive) this.setPhase('speaking')
        break
      case 'output_audio_buffer.stopped':
      case 'output_audio_buffer.cleared':
        if (this.phase === 'speaking') this.setPhase('listening')
        break
      case 'response.created':
        this.responding = true
        break
      case 'response.done':
        this.responding = false
        this.flush()
        break
      case 'conversation.item.input_audio_transcription.completed':
        this.link.add('user', typeof msg.transcript === 'string' ? msg.transcript : '')
        this.link.endTurn()
        break
      case 'response.output_audio_transcript.done':
        this.link.add('voice', typeof msg.transcript === 'string' ? msg.transcript : '')
        this.link.endTurn()
        break
      case 'response.function_call_arguments.done': {
        if (typeof msg.call_id !== 'string' || typeof msg.name !== 'string') return
        let args: Record<string, unknown> = {}
        try {
          if (typeof msg.arguments === 'string') args = JSON.parse(msg.arguments) as Record<string, unknown>
        } catch {}
        const callId = msg.call_id
        // The tool runs on the computer.
        this.toolsRunning++
        void this.link.tool(msg.name, args).then((result) => {
          this.toolsRunning--
          if (this.ended) return
          this.send({ type: 'conversation.item.create', item: { type: 'function_call_output', call_id: callId, output: JSON.stringify(result) } })
          // An empty entry asks for a response once this one is done, so the model speaks the result.
          this.pending.push('')
          this.flush()
        })
        break
      }
      case 'error':
        console.error('realtime:', (msg.error as { message?: string } | undefined)?.message ?? '?')
        break
    }
  }

  private iceChanged(state: RTCIceConnectionState) {
    // While connecting, only a failure counts: the start timer runs until the session is set up.
    if (this.ended || (this.isConnecting && state !== 'failed')) return
    switch (state) {
      case 'connected':
      case 'completed':
        if (this.dropTimer) clearTimeout(this.dropTimer)
        break
      case 'disconnected':
        if (this.dropTimer) clearTimeout(this.dropTimer)
        this.dropTimer = setTimeout(() => this.dropped('Lost the connection to OpenAI.'), 5000)
        break
      case 'failed':
        if (this.connected) this.dropped('Lost the connection to OpenAI.')
        else this.fail("Couldn't reach OpenAI.", 'connection')
        break
    }
  }

  /** Hands waiting replies (and tool results, as empty entries) to the model once it's free. */
  private flush() {
    if (this.responding || this.toolsRunning > 0 || !this.isActive || this.events?.readyState !== 'open' || !this.pending.length) return
    for (const text of this.pending) {
      if (text) this.send({ type: 'conversation.item.create', item: { type: 'message', role: 'user', content: [{ type: 'input_text', text }] } })
    }
    this.pending = []
    this.responding = true
    this.send({ type: 'response.create' })
  }

  private send(event: unknown) {
    if (this.events?.readyState === 'open') this.events.send(JSON.stringify(event))
  }
}
