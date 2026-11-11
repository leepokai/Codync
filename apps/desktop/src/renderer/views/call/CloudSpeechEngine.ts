import { audioType, errorMessage, micPCM16k, onDevicesChanged, SPEECH_CHUNK, spokenText, VoiceEngine, VoiceSettings, wav, type VoiceProvider } from './voice'

export interface CloudSpeech {
  transcribe: (wav: Uint8Array) => Promise<string>
  /** Audio and its format (`aac` or `wav`). */
  speak: (text: string, signal: AbortSignal) => Promise<{ audio: Uint8Array; format: string }>
  stream?: (text: string, signal: AbortSignal) => AsyncIterable<{ audio: Uint8Array; format: string }>
  send: (text: string) => void
}

const THRESHOLD = 0.3
const PREROLL_SAMPLES = (16000 * 3) / 10
/** An utterance has to fit one channel message once base64-encoded. */
const MAX_SAMPLES = 16000 * 20
/** Under 0.4 s of voice is a cough or noise (and speech-to-text models invent words for noise). */
const MIN_VOICED_SAMPLES = (16000 * 4) / 10

/** Sentences grouped into clips of at most `limit` characters (longer sentences are cut). */
export function clips(text: string, limit: number): string[] {
  const out: string[] = []
  let current = ''
  let sentence = ''
  const flush = () => {
    const t = current.trim()
    if (t) out.push(t)
    current = ''
  }
  const add = (piece: string) => {
    if ([...current].length + [...piece].length > limit) flush()
    let rest = [...piece]
    while (rest.length > limit) {
      out.push(rest.slice(0, limit).join(''))
      rest = rest.slice(limit)
    }
    current += rest.join('')
  }
  for (const ch of text) {
    sentence += ch
    if ('.!?。！？\n'.includes(ch)) {
      add(sentence)
      sentence = ''
    }
  }
  add(sentence)
  flush()
  return out
}

/**
 * The provider's speech models without a live conversation: each utterance is cut at a pause,
 * transcribed with the provider's speech-to-text, and sent as an ordinary message; the bot's
 * replies are read aloud with its text-to-speech. Both go through the host, which holds the key.
 */
export class CloudSpeechEngine extends VoiceEngine {
  private stopMic: (() => void) | null = null
  private stopDevices: (() => void) | null = null
  private chunk: number

  // Listening: PCM since speech began (plus a little before), and when it last went quiet.
  private preroll: Int16Array[] = []
  private prerollSamples = 0
  private utterance: Int16Array[] = []
  private utteranceSamples = 0
  private speaking = false
  private quietSince: number | null = null
  private voiced = 0
  /** Playback just ended; the room's echo of it isn't speech. */
  private deafUntil = 0
  /** Transcriptions finish in the order they were spoken. */
  private transcribing: Promise<void> = Promise.resolve()

  // Speaking: the reply's clips, fetched one ahead of the one playing.
  private queue: string[] = []
  private player: HTMLAudioElement | null = null
  private draining = false
  private playbackDone: (() => void) | null = null
  private speakGeneration = 0
  private inFlight = new Set<AbortController>()
  private barge: Int16Array[] = []
  private bargeSamples = 0

  constructor(
    readonly provider: VoiceProvider,
    private speech: CloudSpeech,
  ) {
    super()
    this.chunk = VoiceSettings.localSpeech() ? 120 : SPEECH_CHUNK[provider]
  }

  async start() {
    await this.openMic()
    // A headset plugged in or out: the microphone starts again on what the system picks now.
    if (!this.ended && this.stopMic) this.stopDevices = onDevicesChanged(() => void this.restartMic())
  }

  private async openMic() {
    try {
      const stop = await micPCM16k((pcm, level) => this.heard(pcm, level))
      if (this.ended) return stop()
      this.stopMic = stop
    } catch (e) {
      this.fail(errorMessage(e), 'microphone')
    }
  }

  private async restartMic() {
    if (this.ended || !this.stopMic) return
    this.stopMic()
    this.stopMic = null
    this.dropUtterance()
    if (this.phase === 'listening') this.setPhase('reconnecting')
    await this.openMic()
  }

  end() {
    this.ended = true
    this.onActivityChanged?.(false)
    this.stopSpeaking()
    this.stopDevices?.()
    this.stopDevices = null
    this.stopMic?.()
    this.stopMic = null
  }

  protected mutedChanged(on: boolean) {
    if (on) this.dropUtterance()
  }

  interrupt() {
    if (this.phase !== 'speaking' && !this.inFlight.size && !this.queue.length) return
    this.transcript?.interrupted()
    this.stopSpeaking()
    this.setPhase('listening')
  }

  speak(reply: string) {
    this.say(spokenText(reply))
  }

  announce(notice: string) {
    this.say(notice)
  }

  // MARK: listening

  private heard(pcm: Int16Array, level: number) {
    // Listening once the microphone actually delivers.
    if (!this.ended && this.isConnecting) this.setPhase('listening')
    if (this.ended || this.muted) return
    if (this.phase === 'speaking') {
      this.level = level
      if (level > THRESHOLD + 0.1) { this.barge.push(pcm); this.bargeSamples += pcm.length }
      else { this.barge = []; this.bargeSamples = 0 }
      if (this.bargeSamples < 16000 / 5) return
      const audio = this.barge
      const count = this.bargeSamples
      this.interrupt()
      this.dropUtterance()
      this.preroll = audio
      this.prerollSamples = count
      this.voiced = count
      this.speaking = true
      this.deafUntil = 0
      return
    }
    if (this.phase !== 'listening' || Date.now() < this.deafUntil) return
    this.level = level
    if (level > THRESHOLD) {
      this.speaking = true
      this.quietSince = null
      this.voiced += pcm.length
    } else if (this.speaking && this.quietSince === null) {
      this.quietSince = Date.now()
    }
    if (this.speaking) {
      this.utterance.push(pcm)
      this.utteranceSamples += pcm.length
      const paused = this.quietSince !== null && Date.now() - this.quietSince >= VoiceSettings.pause() * 1000
      if (paused || this.utteranceSamples >= MAX_SAMPLES) this.submit()
    } else {
      this.preroll.push(pcm)
      this.prerollSamples += pcm.length
      while (this.preroll.length > 1 && this.prerollSamples - this.preroll[0]!.length >= PREROLL_SAMPLES) {
        this.prerollSamples -= this.preroll.shift()!.length
      }
    }
  }

  private submit() {
    const audio = wav([...this.preroll, ...this.utterance])
    const enough = this.voiced >= MIN_VOICED_SAMPLES
    this.dropUtterance()
    if (!enough) return
    this.stopSpeaking()
    this.transcribing = this.transcribing.then(async () => {
      try {
        const text = (await this.speech.transcribe(audio)).trim()
        if (!this.ended && text) {
          this.transcript?.add('user', 'recognized', text)
          this.speech.send(text)
        }
      } catch (e) {
        console.error('transcribe:', errorMessage(e))
        if (!this.ended) this.fail(errorMessage(e), 'provider')
      }
    })
  }

  private dropUtterance() {
    this.preroll = []
    this.prerollSamples = 0
    this.utterance = []
    this.utteranceSamples = 0
    this.speaking = false
    this.quietSince = null
    this.voiced = 0
    this.level = 0
    this.barge = []
    this.bargeSamples = 0
  }

  // MARK: speaking

  /** Queues text in clips the provider can turn into one channel message each. */
  private say(text: string) {
    if (this.ended || this.phase === 'failed' || this.phase === 'starting' || !text) return
    this.transcript?.add('assistant', 'queued', text)
    this.queue.push(...clips(text, this.chunk))
    if (!this.draining) this.playNext()
  }

  private playNext() {
    if (this.ended || this.draining) return
    this.draining = true
    const generation = this.speakGeneration
    const controller = new AbortController()
    this.inFlight.add(controller)
    const run = async () => {
      while (this.queue.length && !controller.signal.aborted) {
        const text = this.queue.shift()!
        const fallback = async function* (speech: CloudSpeech) { yield await speech.speak(text, controller.signal) }
        const iterator = (this.speech.stream?.(text, controller.signal) ?? fallback(this.speech))[Symbol.asyncIterator]()
        try {
          let current = await iterator.next()
          while (!current.done && !controller.signal.aborted) {
            const pending = iterator.next().then(value => ({ value }), error => ({ error }))
            await this.play(current.value)
            const result = await pending
            if ('error' in result) throw result.error
            current = result.value
          }
        } finally { await iterator.return?.() }
      }
    }
    void run().catch(e => {
      if (!controller.signal.aborted && !this.ended) this.fail(errorMessage(e), 'provider')
    }).finally(() => {
      this.inFlight.delete(controller)
      if (generation !== this.speakGeneration) return
      this.draining = false
      this.finishedSpeaking()
    })
  }

  private play(clip: { audio: Uint8Array; format: string }): Promise<void> {
    return new Promise((resolve, reject) => {
      const url = URL.createObjectURL(new Blob([clip.audio as BlobPart], { type: audioType(clip.format) }))
      const player = new Audio(url)
      let finished = false
      const done = (error?: Error) => {
        if (finished) return
        finished = true
        URL.revokeObjectURL(url)
        this.playbackDone = null
        this.player = null
        if (error) reject(error); else resolve()
      }
      player.onended = () => done()
      player.onerror = () => done(new Error('Unreadable speech frame'))
      this.playbackDone = () => done()
      this.player = player
      this.dropUtterance()
      this.setPhase('speaking')
      void player.play().catch(e => done(e instanceof Error ? e : new Error(String(e))))
    })
  }

  private finishedSpeaking() {
    this.player = null
    this.deafUntil = Date.now() + 400
    if (this.phase === 'speaking') this.setPhase('listening')
  }

  private stopSpeaking() {
    for (const controller of this.inFlight) controller.abort()
    this.inFlight.clear()
    this.speakGeneration++
    this.queue = []
    this.draining = false
    if (this.player) {
      this.player.onended = null
      this.player.onerror = null
      this.player.pause()
      URL.revokeObjectURL(this.player.src)
    }
    this.playbackDone?.()
    this.playbackDone = null
    this.player = null
  }
}
