import type { HostClient } from '../../client/host-client'
import { pref, type Pref } from '../../lib/prefs'
import { requireLiveHost } from '../call/openai-live-host'

// Realtime voice on the user's own provider keys (host/src/voice.rs, kit Voice.swift +
// VoiceSettings). The keys stay on the computer; these are this device's choices.

export type VoiceProvider = 'openai' | 'gemini'
export const VOICE_PROVIDERS: VoiceProvider[] = ['openai', 'gemini']

/** The provider's newest model for each voice mode, as the computer last found them. */
export interface VoiceDefaults {
  realtime: string
  transcribe: string
  speech: string
}

export interface VoiceStatus {
  providers: { provider: VoiceProvider; configured: boolean; defaults?: VoiceDefaults | null }[]
}

export interface VoiceModels {
  /** Realtime conversation models. */
  models: string[]
  transcribe: string[]
  speech: string[]
  defaults?: VoiceDefaults | null
}

export const voiceInfo: Record<VoiceProvider, {
  name: string
  keyPage: string
  keyPrompt: string
  /** Used until the user picks one, and until the computer has said what's newest. */
  defaults: VoiceDefaults
  voices: string[]
}> = {
  openai: {
    name: 'OpenAI',
    keyPage: 'https://platform.openai.com/api-keys',
    keyPrompt: 'sk-…',
    defaults: { realtime: 'gpt-realtime-2.1', transcribe: 'gpt-transcribe', speech: 'gpt-4o-mini-tts' },
    voices: ['marin', 'cedar', 'alloy', 'ash', 'ballad', 'coral', 'echo', 'sage', 'shimmer', 'verse'],
  },
  gemini: {
    name: 'Gemini',
    keyPage: 'https://aistudio.google.com/apikey',
    keyPrompt: 'AIza…',
    defaults: { realtime: 'gemini-3.8-live', transcribe: 'gemini-3.8-flash', speech: 'gemini-3.8-flash-tts' },
    voices: ['Kore', 'Puck', 'Charon', 'Fenrir', 'Aoede', 'Leda', 'Orus', 'Zephyr'],
  },
}

export const isVoiceProvider = (s: string): s is VoiceProvider => s === 'openai' || s === 'gemini'
export const configured = (s: VoiceStatus, p: VoiceProvider) => s.providers.some((x) => x.provider === p && x.configured)

/** `device` (on-device speech) or a provider. */
export const engine = pref('callEngine', 'device')
/** The same choice for dictation into the message box, picked beside its mic. */
export const dictationEngine = pref('dictationEngine', 'device')
/** On-device reading speed (AVSpeechUtterance's scale: 0.5 is normal). */
export const DEFAULT_RATE = 0.5
export const rate = pref('callRate', DEFAULT_RATE)
/** Seconds of silence that end what you say. */
export const pause = pref('callPause', 1.5)
/** On-device recognition language (a locale identifier); '' follows the system. */
export const language = pref('callLanguage', '')

/** Picker options: the system's language, then every language the recognizer supports, by name. */
export function languageOptions(supported: string[]) {
  const names = new Intl.DisplayNames([navigator.language], { type: 'language' })
  const name = (id: string) => {
    try {
      return names.of(id.replace('_', '-')) ?? id
    } catch {
      return id
    }
  }
  return [
    { id: '', label: 'System' },
    ...supported.map((id) => ({ id, label: name(id) })).sort((a, b) => a.label.localeCompare(b.label)),
  ]
}

const cache = new Map<string, Pref<unknown>>()
function keyed<T>(key: string, fallback: T): Pref<T> {
  if (!cache.has(key)) cache.set(key, pref<T>(key, fallback) as Pref<unknown>)
  return cache.get(key) as Pref<T>
}

/** `realtime` (a live conversation) or `speech` (speech to text, then the reply read aloud). */
export const mode = (p: VoiceProvider) => keyed<string>(`callMode.${p}`, 'realtime')
const voicePref = (p: VoiceProvider) => keyed<string | null>(`callVoice.${p}`, null)
const defaultsPref = (p: VoiceProvider) => keyed<VoiceDefaults | null>(`callDefaults.${p}`, null)
/** The provider's model list as last fetched with the user's key. */
export const modelsCache = (p: VoiceProvider) => keyed<VoiceModels | null>(`callModels.${p}`, null)
const modelPrefs = {
  realtime: (p: VoiceProvider) => keyed<string | null>(`callModel.${p}`, null),
  transcribe: (p: VoiceProvider) => keyed<string | null>(`callSpeechToText.${p}`, null),
  speech: (p: VoiceProvider) => keyed<string | null>(`callSpeechModel.${p}`, null),
}

/** The newest models as the computer last reported them, or the app's own until it has. */
export const voiceDefaults = (p: VoiceProvider) => defaultsPref(p).get() ?? voiceInfo[p].defaults

export function rememberDefaults(p: VoiceProvider, d: VoiceDefaults | null | undefined) {
  if (d) defaultsPref(p).set(d)
}

export type ModelKind = keyof VoiceDefaults

export const chosenModel = (p: VoiceProvider, kind: ModelKind) => modelPrefs[kind](p).get() ?? voiceDefaults(p)[kind]

/** Keeps a model the user picked; the default isn't stored, so a newer default reaches them. */
export function saveModel(p: VoiceProvider, kind: ModelKind, model: string) {
  modelPrefs[kind](p).set(!model || model === voiceDefaults(p)[kind] ? null : model)
}

export function chosenVoice(p: VoiceProvider) {
  const v = voicePref(p).get()
  return v && voiceInfo[p].voices.includes(v) ? v : voiceInfo[p].voices[0]!
}

export const saveVoice = (p: VoiceProvider, v: string) => voicePref(p).set(v)

// Minutes of realtime voice used this month, counted on this device: an estimate, not a bill.
const usageKey = (p: VoiceProvider, d: Date) => `callUsage.${p}.${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`

export function addVoiceUsage(seconds: number, p: VoiceProvider, at = new Date()) {
  if (seconds <= 0) return
  const u = keyed<number>(usageKey(p, at), 0)
  u.set(u.get() + seconds)
}

export const voiceMinutes = (p: VoiceProvider, at = new Date()) => Math.floor((keyed<number>(usageKey(p, at), 0).get() + 59) / 60)

// Host calls

/** Which providers the computer has keys for; the newest models it reports become the defaults. */
export async function voiceStatus(c: HostClient) {
  const status = await c.call<VoiceStatus>('voiceStatus')
  for (const entry of status.providers) rememberDefaults(entry.provider, entry.defaults)
  return status
}

/** Checks the key with the provider before the computer keeps it; an empty key removes it. */
export const setVoiceKey = (c: HostClient, p: VoiceProvider, key: string) => c.call<VoiceStatus>('setVoiceKey', { provider: p, key }, 30_000)

/** Validate access. Realtime also returns a short-lived credential; Live does not create a billed session. */
export const voiceSession = async (c: HostClient, p: VoiceProvider, model: string, voice: string) => {
  if (p === 'openai' && model.startsWith('gpt-live-')) await requireLiveHost(c)
  return (await c.call<{ credential?: string; validated?: boolean }>('voiceSession', { provider: p, model, voice }, 30_000)).credential
}

/** The models the computer's key can use, per voice mode. */
export const voiceModels = (c: HostClient, p: VoiceProvider) => c.call<VoiceModels>('voiceModels', { provider: p, supportsLive: true }, 30_000)

/** Text to speech for a stretch of a reply: base64 audio and its format (`aac` or `wav`). */
export const voiceSpeak = (c: HostClient, p: VoiceProvider, model: string, voice: string, text: string) =>
  c.call<{ audio: string; format: string }>('voiceSpeak', { provider: p, model, voice, text }, 90_000)
