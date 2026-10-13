import { Observable } from '../../lib/observable'
import {
  addVoiceUsage,
  chosenModel,
  chosenVoice,
  engine,
  isVoiceProvider,
  language,
  mode,
  pause,
  rate,
  rememberStatus,
  voiceInfo,
  type VoiceProvider,
} from '../settings/voice-settings'

// Calls use the voice settings the settings view keeps (views/settings/voice-settings.ts).

export type { VoiceProvider, VoiceStatus } from '../settings/voice-settings'

export const providerName = (p: VoiceProvider) => voiceInfo[p].name

/** Longest stretch of a reply turned into audio at once: each clip has to fit one channel message. Gemini returns PCM, OpenAI AAC. */
export const SPEECH_CHUNK: Record<VoiceProvider, number> = { openai: 600, gemini: 150 }

/** Recognition through the Swift helper: macOS only. */
export const onDeviceAvailable = () => window.codync.speech.available
export const onDeviceUnavailableReason = () =>
  window.codync.platform === 'darwin'
    ? "On-device speech isn't available in this build."
    : `On-device speech isn't available on ${window.codync.platform === 'win32' ? 'Windows' : 'Linux'}.`

export const VoiceSettings = {
  /** The provider picked in the call settings; null for on-device. */
  provider: (): VoiceProvider | null => {
    const value = engine.get()
    return isVoiceProvider(value) ? value : null
  },
  model: (p: VoiceProvider) => chosenModel(p, 'realtime'),
  transcribeModel: (p: VoiceProvider) => chosenModel(p, 'transcribe'),
  speechModel: (p: VoiceProvider) => chosenModel(p, 'speech'),
  voice: chosenVoice,
  isSpeechMode: (p: VoiceProvider) => mode(p).get() === 'speech',
  remember: rememberStatus,
  /** A pause this long (seconds) ends what you're saying and sends it. */
  pause: () => (pause.get() > 0 ? pause.get() : 1.5),
  /** AVSpeechUtterance's rate (0…1, 0.5 normal), as the Mac stores it. */
  rate: () => rate.get(),
  /** The on-device recognizer's language; '' is the computer's. */
  language: () => language.get(),
}

/** This month's realtime minutes, counted on this device. */
export const countVoiceUsage = (seconds: number, p: VoiceProvider) => addVoiceUsage(Math.floor(seconds), p)

// MARK: engines

export type VoicePhase = 'starting' | 'listening' | 'speaking' | 'failed'

/**
 * What carries a call's audio: on-device speech or a realtime model on the user's own key.
 * `CallView` only talks to this; nothing on the host changes.
 */
export abstract class VoiceEngine extends Observable {
  phase: VoicePhase = 'starting'
  failure: string | null = null
  /** Microphone loudness, 0…1, while listening. Read every frame; not observed. */
  level = 0
  abstract readonly provider: VoiceProvider | null
  /** Keeps the call subscribed to replies while audio is active. */
  onActivityChanged: ((active: boolean) => void) | null = null
  protected ended = false
  private mutedValue = false

  get isActive() {
    return this.phase === 'listening' || this.phase === 'speaking'
  }

  get muted() {
    return this.mutedValue
  }
  set muted(on: boolean) {
    if (on === this.mutedValue) return
    this.mutedValue = on
    this.mutedChanged(on)
    this.changed()
  }
  protected mutedChanged(_on: boolean) {}

  protected setPhase(phase: VoicePhase) {
    this.phase = phase
    this.changed()
    this.onActivityChanged?.(!this.ended && this.isActive)
  }

  protected fail(message: string) {
    this.end()
    this.failure = message
    this.setPhase('failed')
  }

  abstract start(): Promise<void>
  /** Idempotent. */
  abstract end(): void
  /** Cuts what's being said short. */
  abstract interrupt(): void
  /** A new final reply from the bot to say. */
  abstract speak(reply: string): void
  /** A notice from the computer (an approval is waiting) to say. */
  abstract announce(notice: string): void
}

export const errorMessage = (e: unknown) => (e instanceof Error ? e.message : String(e))
export const MIC_DENIED = 'Allow the microphone for Codync in Settings to talk to your bots.'

// MARK: audio helpers

/** The microphone, echo-cancelled so the bot can be talked over. */
export async function microphone(): Promise<MediaStream> {
  try {
    return await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 } })
  } catch (e) {
    if (e instanceof DOMException && (e.name === 'NotAllowedError' || e.name === 'SecurityError')) throw new Error(MIC_DENIED)
    throw new Error(`Couldn't use the microphone: ${errorMessage(e)}`)
  }
}

/**
 * Microphone → 16 kHz mono PCM16 with a loudness level, 0…1 (kit `MicTap.pcm16k`). Returns the
 * function that stops it.
 */
export async function micPCM16k(out: (pcm: Int16Array, level: number) => void): Promise<() => void> {
  const stream = await microphone()
  const ctx = new AudioContext({ sampleRate: 16000 })
  const source = ctx.createMediaStreamSource(stream)
  // ponytail: ScriptProcessor is deprecated but needs no worklet file (the CSP allows only 'self'); AudioWorklet if it's ever removed.
  const node = ctx.createScriptProcessor(2048, 1, 1)
  node.onaudioprocess = (e) => {
    const input = e.inputBuffer.getChannelData(0)
    const pcm = new Int16Array(input.length)
    let sum = 0
    for (let i = 0; i < input.length; i++) {
      const v = Math.max(-1, Math.min(1, input[i]!))
      pcm[i] = v < 0 ? v * 32768 : v * 32767
      sum += v * v
    }
    out(pcm, levelOf(sum, input.length))
  }
  source.connect(node)
  node.connect(ctx.destination)
  if (ctx.state === 'suspended') await ctx.resume()
  return () => {
    node.onaudioprocess = null
    source.disconnect()
    node.disconnect()
    for (const t of stream.getTracks()) t.stop()
    void ctx.close()
  }
}

/** -50 dB (quiet room) … 0 dB mapped to 0…1. */
export function levelOf(sumOfSquares: number, count: number) {
  const db = 10 * Math.log10(Math.max(sumOfSquares / Math.max(count, 1), 1e-10))
  return Math.min(1, Math.max(0, (db + 50) / 50))
}

/** 16 kHz mono PCM16 wrapped in a WAV header. */
export function wav(chunks: Int16Array[]): Uint8Array {
  const samples = chunks.reduce((n, c) => n + c.length, 0)
  const out = new Uint8Array(44 + samples * 2)
  const v = new DataView(out.buffer)
  const ascii = (at: number, s: string) => [...s].forEach((c, i) => v.setUint8(at + i, c.charCodeAt(0)))
  ascii(0, 'RIFF')
  v.setUint32(4, 36 + samples * 2, true)
  ascii(8, 'WAVEfmt ')
  v.setUint32(16, 16, true)
  v.setUint16(20, 1, true)
  v.setUint16(22, 1, true)
  v.setUint32(24, 16000, true)
  v.setUint32(28, 32000, true)
  v.setUint16(32, 2, true)
  v.setUint16(34, 16, true)
  ascii(36, 'data')
  v.setUint32(40, samples * 2, true)
  let at = 44
  for (const c of chunks) for (let i = 0; i < c.length; i++, at += 2) v.setInt16(at, c[i]!, true)
  return out
}

export function toBase64(bytes: Uint8Array) {
  let s = ''
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
  return btoa(s)
}

export function fromBase64(text: string) {
  const s = atob(text)
  const out = new Uint8Array(s.length)
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i)
  return out
}

// MARK: spoken text

/** A chat reply as it should sound: markdown syntax and code blocks dropped (kit SpokenText). */
export function spokenText(markdown: string) {
  const rules: [RegExp, string][] = [
    [/```[\s\S]*?```/g, ''], // code blocks
    [/!?\[([^\]]*)\]\([^)]*\)/g, '$1'], // links and images → their text
    [/`([^`]*)`/g, '$1'], // inline code
    [/^\s{0,3}(#{1,6}|>|[-*+]|\d+\.)\s+/gm, ''], // headings, quotes, list markers
    [/(\*\*|\*|~~)(\S[^\n]*?)\1/g, '$2'], // emphasis (not `_`: it's in identifiers)
    [/^[ \t]*\|?[ \t:|-]{3,}\|?[ \t]*$/gm, ''], // table rules
    [/^[ \t]*\|[ \t]*|[ \t]*\|[ \t]*$/gm, ''], // table edges
    [/[ \t]*\|[ \t]*/g, ', '], // table cells
    [/\n{2,}/g, '\n'],
  ]
  let text = markdown
  for (const [pattern, template] of rules) text = text.replace(pattern, template)
  return text.trim()
}
