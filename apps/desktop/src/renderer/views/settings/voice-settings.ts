import { dictationEngineIds, voiceEngine, voiceEngineIds } from '../../../shared/voice-engine'
import type { HostClient, HostError } from '../../client/host-client'
import { trNow } from '../../lib/l10n'
import { pref, type Pref } from '../../lib/prefs'

// Realtime voice on the user's own provider keys (host/src/voice.rs, kit Voice.swift +
// VoiceSettings). The keys stay on the computer; these are this device's choices.

export type VoiceProvider = 'openai' | 'gemini'
export const VOICE_PROVIDERS: VoiceProvider[] = ['openai', 'gemini']

/** What transcribes dictation in the message box: a provider on the user's key, or `local`, the speech server linked on the computer (no key, no usage). */
export type DictationTarget = VoiceProvider | 'local'

/** The provider's newest model for each voice mode, as the computer last found them. */
export interface VoiceDefaults {
  realtime: string
  transcribe: string
  speech: string
}

export interface VoiceStatus {
  /** Absent on hosts that cannot configure search for a call. */
  googleSearch?: boolean
  /**
   * `server`: the OpenAI-compatible server the computer sends this engine's Speech mode to, `''` for the provider
   * itself. Absent where there is none to choose: Gemini, a cloud computer, an older host.
   */
  providers: { provider: VoiceProvider; configured: boolean; defaults?: VoiceDefaults | null; server?: string }[]
}

/** Whether the computer's linked speech server can transcribe dictation; `message` says what to do when it can't. */
export interface LocalDictation {
  ready: boolean
  message: string | null
  /** The linked speech server's address; hosts before `server` omit it. */
  server?: string | null
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
/** What the Server row reads: the saved address, or OpenAI's own (only the OpenAI engine has the row). */
export const serverLabel = (saved: string) => saved || 'api.openai.com'

/** `device` (on-device speech), `local` (the computer's speech model) or a provider. */
function enginePref(stored: Pref<string>): Pref<string> {
  return { ...stored, get: () => voiceEngine(stored.get(), window.codync.platform) }
}

export const engineOptions = () => voiceEngineIds(window.codync.platform).map((id) => ({
  id, label: isVoiceProvider(id) ? voiceInfo[id].name : 'On device',
}))

/** The `local` engine goes by the computer it runs on: "This computer" when that's where this app is, its name otherwise (an SSH or account computer). */
export const localEngineName = (thisComputer: boolean, hostName: string) => (thisComputer ? 'This computer' : hostName)

/** The engines the message box dictates with: call engines plus `local`, under `localName` (dictation only). */
export const dictationEngineOptions = (localName: string) =>
  dictationEngineIds(window.codync.platform).map((id) => ({
    id,
    label: isVoiceProvider(id) ? voiceInfo[id].name : id === 'local' ? localName : 'On device',
  }))

const storedCallEngine = pref('callEngine', 'device')
/** Keep an old/default device choice available to call startup's configured-provider fallback. */
export const callEngineChoice = storedCallEngine.get
export const engine = enginePref(storedCallEngine)
/** The same choice for dictation into the message box, picked beside its mic. */
export const dictationEngine = enginePref(pref('dictationEngine', 'device'))
/** On-device reading speed (AVSpeechUtterance's scale: 0.5 is normal). */
export const DEFAULT_RATE = 0.5
export const rate = pref('callRate', DEFAULT_RATE)
/** Seconds of silence that end what you say. */
export const pause = pref('callPause', 1.5)
/** On-device recognition language (a locale identifier); '' follows the system. */
export const language = pref('callLanguage', '')
/** Google's built-in search in Gemini realtime calls; off until this device opts in. */
export const googleSearch = pref('callGoogleSearch.gemini', false)

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
/** The server `p`'s Speech mode goes to, as the computer last said: `''` for the provider itself, null where it has none to choose. */
export const server = (p: VoiceProvider) => keyed<string | null>(`callServer.${p}`, null)
/** A server of the user's own holds no Realtime conversation, so Speech is its only mode; the picked mode comes back with the provider. */
export const speechOnly = (savedServer: string | null) => Boolean(savedServer)
export const isSpeechMode = (p: VoiceProvider) => speechOnly(server(p).get()) || mode(p).get() === 'speech'
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

/**
 * Keeps what the computer says of each provider: its newest models become the defaults, and its server decides the
 * mode. Another server has other models: the ones picked, and the list, for the one before go.
 */
function remember(status: VoiceStatus) {
  for (const entry of status.providers) {
    const p = entry.provider
    if ((server(p).get() ?? '') !== (entry.server ?? '')) {
      for (const kind of Object.values(modelPrefs)) kind(p).set(null)
      modelsCache(p).set(null)
    }
    rememberDefaults(p, entry.defaults)
    server(p).set(entry.server ?? null)
  }
  return status
}

/** Which providers the computer has keys for, and the server each one's Speech mode goes to. */
export const voiceStatus = async (c: HostClient) => remember(await c.call<VoiceStatus>('voiceStatus'))

/** The linked speech server and whether dictation can use it; hosts before `localDictation` are told to update. */
export async function localDictation(c: HostClient): Promise<LocalDictation> {
  try {
    return await c.call<LocalDictation>('localDictation')
  } catch (e) {
    if ((e as Partial<HostError>).status === 404) return { ready: false, message: trNow('Update Codync on your computer to dictate with its own speech model.') }
    throw e
  }
}

/** Checks the key with the provider before the computer keeps it; an empty key removes it. */
export const setVoiceKey = async (c: HostClient, p: VoiceProvider, key: string) =>
  remember(await c.call<VoiceStatus>('setVoiceKey', { provider: p, key }, 30_000))

/** Points the engine's Speech mode at an OpenAI-compatible server, or back at the provider (empty). The computer checks a saved key with it first. */
export const setVoiceServer = async (c: HostClient, p: VoiceProvider, url: string) =>
  remember(await c.call<VoiceStatus>('setVoiceServer', { provider: p, url }, 30_000))

/** The computer gives the speech server two minutes per request (host `voice::local` `TIMEOUT`) — linking's test piece and every dictated piece, a big model on a slow CPU — plus the trip there and back. */
export const SPEECH_SERVER_MS = 135_000

/**
 * Links the speech server at `url`, asking it for `model` (empty: the one it serves), after the computer checks it with a test
 * transcription; an empty `url` unlinks. Loopback-only, like `setVoiceKey`.
 */
export const setSpeechServer = (c: HostClient, url: string, model = '') =>
  c.call<LocalDictation>('setSpeechServer', { url, model }, SPEECH_SERVER_MS)

/** A short-lived credential for one call's audio session. */
export const voiceSession = async (c: HostClient, p: VoiceProvider, model: string, voice: string) =>
  (await c.call<{ credential: string }>('voiceSession', { provider: p, model, voice }, 30_000)).credential

/** The models the computer's key can use, per voice mode. */
export const voiceModels = (c: HostClient, p: VoiceProvider) => c.call<VoiceModels>('voiceModels', { provider: p }, 30_000)

/** Text to speech for a stretch of a reply: base64 audio and its format (`aac` or `wav`). */
export const voiceSpeak = (c: HostClient, p: VoiceProvider, model: string, voice: string, text: string) =>
  c.call<{ audio: string; format: string }>('voiceSpeak', { provider: p, model, voice, text }, 90_000)
