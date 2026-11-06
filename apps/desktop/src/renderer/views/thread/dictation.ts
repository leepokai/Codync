import type { CodyncBridge, SpeechEvent } from '../../../shared/ipc.ts'
import { VoiceKeyRouter, type VoiceKey } from '../../../shared/voice-shortcut.ts'
import { Observable } from '../../lib/observable.ts'
import type { VoiceProvider } from '../settings/voice-settings.ts'
import { inOrder, joinDictated, MIN_SAMPLES, pieces } from './dictation-pieces.ts'

/** The running dictation: one recognizer at a time, and a call stops dictation first. */
let active: Dictation | null = null

export function stopDictation() {
  active?.cancel()
}

/** What dictation reaches outside itself: `dictation-platform.ts` wires the real ones, tests stub them. */
export interface DictationPlatform {
  /** The on-device speech helper (macOS), or null without one. */
  speech: Pick<CodyncBridge['speech'], 'start' | 'stop' | 'onEvent'> | null
  /** Why there's no on-device speech here, said when it's picked anyway. */
  noSpeech: string
  /** This window reaches a host, so it can dictate with a provider on the user's key. */
  host: boolean
  /** The on-device recognizer's language; null is the computer's. */
  language(): string | null
  /** The provider picked beside the mic; null is on-device. */
  provider(): VoiceProvider | null
  /** Whether the host holds the provider key. Network failures still reject. */
  checkKey(provider: VoiceProvider): Promise<boolean>
  configureKey(provider: VoiceProvider): void
  /** Opens the microphone as 16 kHz PCM; resolves to what closes it. */
  microphone(out: (pcm: Int16Array, level: number) => void): Promise<() => void>
  /** One piece of a recording, as the provider's text. */
  transcribe(provider: VoiceProvider, piece: Int16Array): Promise<string>
  /** Seconds sent to `provider`, for this month's usage. */
  count(seconds: number, provider: VoiceProvider): void
  /** Shows an error, every time (even the same one again). */
  report(message: string): void
}

/** Recording for a provider: what the microphone heard so far, and how to stop it. */
interface Recording {
  provider: VoiceProvider
  pcm: Int16Array[]
  stopMic: (() => void) | null
}

const messageOf = (e: unknown) => (e instanceof Error ? e.message : String(e))

/**
 * Dictation into a message box. With on-device speech (the speech helper; macOS), words reach
 * `edit` as they're recognized, after what was already typed; the helper doesn't stop on a pause:
 * it ends on `finish()` once the last words are in, on `cancel()`, on an error, or when another
 * window takes it. With a provider picked beside the mic, it records, and `finish()` has the host
 * transcribe the whole recording with the user's key, then puts it after what's in the box by then.
 */
export class Dictation extends Observable {
  listening = false
  /** Recorded for a provider, waiting for its transcript. */
  transcribing = false
  /** When it started listening (ms since the epoch). */
  startedAt = 0
  /** The microphone's level while listening, 0…1. Read it each frame: it doesn't re-render. */
  level = 0
  /** Why the last dictation ended early (no microphone, speech recognition not allowed), until the next. */
  error: string | null = null
  /** The box up to the words still being recognized. */
  private before = ''
  private off: (() => void) | null = null
  private recording: Recording | null = null
  private done: (() => void) | null = null
  /** Bumped by each start, finish and cancel: a microphone or transcript arriving late for an earlier one is dropped. */
  private generation = 0
  /** Started by the voice input shortcut's double tap: its next tap sends what's in the field. */
  private byShortcut = false
  private readonly keys = new VoiceKeyRouter()
  /** Changes the box's text. */
  private readonly edit: (change: (box: string) => string) => void
  private readonly platform: DictationPlatform

  constructor(edit: (change: (box: string) => string) => void, platform: DictationPlatform) {
    super()
    this.edit = edit
    this.platform = platform
  }

  /** On-device speech here, or a host to transcribe with a provider. */
  get available() {
    return this.platform.speech !== null || this.platform.host
  }

  /** Listening, or stopped and waiting for the last words. */
  get busy() {
    return this.off !== null || this.recording !== null || this.transcribing
  }

  start(draft: string, byShortcut = false) {
    active?.cancel()
    active = this
    this.byShortcut = byShortcut
    this.error = null
    this.before = draft
    const { speech } = this.platform
    const provider = this.platform.host ? this.platform.provider() : null
    if (provider) {
      this.recording = { provider, pcm: [], stopMic: null }
      return void this.record(this.recording, ++this.generation)
    }
    if (!speech) return this.fail(`${this.platform.noSpeech} Pick OpenAI or Gemini beside the mic.`)
    this.off = speech.onEvent((e) => this.event(e))
    speech.start(this.platform.language())
    this.listen()
  }

  /** Stops listening; `done` runs once the last words are in the box (recognized, or transcribed for a provider). */
  finish(done?: () => void) {
    this.byShortcut = false
    if (this.recording) return void this.transcribe(this.recording, done)
    if (!this.off) return done?.()
    this.done = done ?? null
    this.listening = false
    this.level = 0
    this.changed()
    this.platform.speech?.stop()
  }

  /** Stops listening now and drops words still on their way. */
  cancel() {
    this.byShortcut = false
    this.error = null
    if (!this.busy) return
    this.generation++
    if (this.off) this.platform.speech?.stop()
    this.done = null
    this.end()
  }

  /**
   * A tap of the voice input shortcut: a tap after its double tap finishes and runs `send`. True
   * when it's a double tap that should start: the caller shows its field, then `start(draft, true)`.
   */
  shortcutKey(key: VoiceKey, send: () => void): boolean {
    const action = this.keys.route(key, { byShortcut: this.byShortcut, busy: this.busy })
    if (action === 'send') this.finish(send)
    return action === 'start'
  }

  private event(e: SpeechEvent) {
    if (e.type === 'level') this.level = e.value
    if (e.type === 'partial' || e.type === 'final') this.edit(() => joinDictated(this.before, e.text))
    if (e.type === 'final') this.before = joinDictated(this.before, e.text)
    if (e.type === 'error') {
      // Nothing to send, and the helper (which may still hold the microphone) goes too.
      this.byShortcut = false
      this.error = e.message
      this.platform.report(e.message)
      this.platform.speech?.stop()
    }
    if (e.type === 'stopped' || e.type === 'error') this.end()?.()
  }

  /**
   * Opens the microphone once the host has the provider's key, so nothing said is lost to a missing
   * one, and only then listens (the pill shows), so nothing said before the microphone is open is lost.
   */
  private async record(recording: Recording, generation: number) {
    try {
      const hasKey = await this.platform.checkKey(recording.provider)
      if (generation !== this.generation) return
      if (!hasKey) {
        this.end()
        this.platform.configureKey(recording.provider)
        return
      }
      const stop = await this.platform.microphone((pcm, level) => {
        recording.pcm.push(pcm)
        this.level = level
      })
      if (generation !== this.generation) return stop()
      recording.stopMic = stop
      this.listen()
    } catch (e) {
      if (generation === this.generation) this.fail(messageOf(e))
    }
  }

  /**
   * Stops recording and puts the provider's transcript of all of it after what's in the box now
   * (typing goes on while it records); `done` runs once it's there.
   */
  private async transcribe(recording: Recording, done?: () => void) {
    const generation = ++this.generation
    recording.stopMic?.()
    this.recording = null
    this.done = done ?? null
    this.listening = false
    this.level = 0
    const samples = recording.pcm.reduce((n, c) => n + c.length, 0)
    if (samples < MIN_SAMPLES) return void this.end()?.()
    this.transcribing = true
    this.changed()
    try {
      const { provider } = recording
      // Once cancelled, the pieces not sent yet stay unsent (each is billed on the user's key).
      const texts = await inOrder(pieces(recording.pcm), (piece) =>
        generation === this.generation ? this.platform.transcribe(provider, piece) : Promise.reject(new Error('Cancelled')),
      )
      this.platform.count(samples / 16000, provider)
      if (generation !== this.generation) return
      const said = texts.map((t) => t.trim()).filter(Boolean).reduce(joinDictated, '')
      this.edit((box) => joinDictated(box, said))
      this.end()?.()
    } catch (e) {
      if (generation === this.generation) this.fail(messageOf(e))
    }
  }

  /** Shows it listening, timed from now. */
  private listen() {
    this.startedAt = Date.now()
    this.listening = true
    this.changed()
  }

  /** Ends with `message` and nothing to send. */
  private fail(message: string) {
    this.byShortcut = false
    this.error = message
    this.platform.report(message)
    this.done = null
    this.end()
  }

  /** Returns what was waiting for the last words. */
  private end() {
    this.off?.()
    this.off = null
    this.recording?.stopMic?.()
    this.recording = null
    this.transcribing = false
    this.listening = false
    this.level = 0
    if (active === this) active = null
    const done = this.done
    this.done = null
    this.changed()
    return done
  }
}
