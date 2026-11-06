import type { VoiceCallEngine } from '@shared/analytics'
import type { CallCue } from '@shared/calls'
import { isWorking } from '@shared/models'
import { createContext, useContext, useEffect, useRef, useState } from 'react'
import { BotAvatar } from '../../components/Avatar'
import { Icon } from '../../components/Icon'
import { LevelDots } from '../../components/LevelDots'
import { Sheet } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { avatarColor } from '../../lib/theme'
import { analytics } from '../../store/analytics'
import { useStore } from '../../store/context'
import { VoiceChatSettingsView } from '../settings/VoiceChatSettingsView'
import { engine as preferredEngine, voiceStatus } from '../settings/voice-settings'
import { stopDictation } from '../thread/dictation'
import { CallLink } from './call-link'
import { CallQuality } from './call-quality'
import { CloudSpeechEngine } from './CloudSpeechEngine'
import { GeminiLiveEngine } from './GeminiLiveEngine'
import { OnDeviceEngine } from './OnDeviceEngine'
import { OpenAIRealtimeEngine } from './OpenAIRealtimeEngine'
import { countVoiceUsage, fromBase64, onDeviceAvailable, onDeviceUnavailableReason, providerName, transcribeOnHost, VoiceEngine, VoiceSettings, voiceSettingsClosed, type VoicePhase, type VoiceProvider } from './voice'
import './call.css'

/** What the call bar says for `phase`. */
function statusText(phase: VoicePhase | null, muted: boolean, working: boolean) {
  switch (phase) {
    case null:
    case 'starting':
      return 'Connecting'
    case 'reconnecting':
      return 'Reconnecting'
    case 'speaking':
      return 'Speaking'
    case 'failed':
      return "Can't listen"
    case 'listening':
      return muted ? 'Muted' : working ? 'Working, listening' : 'Listening'
  }
}

/**
 * Opens the window's own Settings at Voice chat; without it (no Settings screen around) the
 * call bar shows the voice settings in their own sheet.
 */
export const OpenVoiceSettings = createContext<(() => void) | null>(null)

/**
 * A voice call with a bot, as Grok Bot does it: a floating bar over the chat, which stays
 * readable underneath. The computer runs the call (`CallLink`): what you say reaches the bot as
 * the call's messages, its final replies are read aloud, and the call ends as one titled line
 * ("Release plan · 00:16") that opens its conversation.
 */
export function CallView({ botId, onSpeaking, interrupt, onEnd }: {
  botId: string
  onSpeaking: (speaking: boolean) => void
  interrupt: { current: (() => void) | null }
  onEnd: () => void
}) {
  const store = useStore()
  const [session, setSession] = useState<VoiceEngine | null>(null)
  useModel(session)
  /** One line under the bar, e.g. why the call fell back to another voice. */
  const [notice, setNotice] = useState<string | null>(null)
  /** No engine could start at all. */
  const [blocked, setBlocked] = useState<string | null>(null)
  const [settings, setSettings] = useState(false)
  const openVoiceSettings = useContext(OpenVoiceSettings)
  const engine = useRef<VoiceEngine | null>(null)
  /** When the current cloud engine started, for this month's minutes. */
  const cloudStartedAt = useRef<number | null>(null)
  /** The call settings the current engine was built from. */
  const engineChoice = useRef('')
  const closed = useRef(false)
  /** What carries the current engine's audio, and whether the call has been counted (once its first engine runs). */
  const engineName = useRef<VoiceCallEngine>('on_device')
  const counted = useRef(false)
  const bot = store.bots.get(botId)
  const color = bot ? avatarColor(bot.avatarColor) : null
  /** The call on the computer, which runs it. */
  const link = useRef<CallLink | null>(null)
  link.current ??= new CallLink(store, botId, crypto.randomUUID())
  /** Each engine's `voice_call_quality`. */
  const quality = useRef<CallQuality | null>(null)
  quality.current ??= new CallQuality((event) => analytics.track(event))

  const countCloudMinutes = () => {
    const provider = engine.current?.provider
    if (!provider || cloudStartedAt.current === null) return
    countVoiceUsage((Date.now() - cloudStartedAt.current) / 1000, provider)
    cloudStartedAt.current = null
  }

  /** Starts the engine: a realtime provider on the key kept by the computer, or on-device speech. */
  const begin = (provider: VoiceProvider | null) => {
    if (closed.current) return
    // Every engine takes the microphone (and the speech helper) from any dictation in progress.
    stopDictation()
    engineChoice.current = VoiceSettings.engineChoice(provider)
    if (!provider && !onDeviceAvailable()) return void fallbackFromDevice()
    const call = link.current!
    const send = (text: string) => store.send(text, botId, { call: call.call })
    let next: VoiceEngine
    if (provider && VoiceSettings.isSpeechMode(provider)) {
      const transcribe = VoiceSettings.transcribeModel(provider)
      const speechModel = VoiceSettings.speechModel(provider)
      const voice = VoiceSettings.voice(provider)
      next = new CloudSpeechEngine(provider, {
        // ponytail: kit turns Simplified Chinese transcripts into Traditional (ICU Hans-Hant); Chromium has no such transform.
        transcribe: async (audio) => transcribeOnHost(await store.ready(), provider, transcribe, audio),
        speak: async (text) => {
          const clip = await (await store.ready()).call<{ audio: string; format: string }>('voiceSpeak', { provider, model: speechModel, voice, text }, 90_000)
          return { audio: fromBase64(clip.audio), format: clip.format }
        },
        send,
      })
      // The computer keeps the call, though it carries no voice model.
      void call.start('speech', provider).catch(() => {})
      cloudStartedAt.current = Date.now()
      engineName.current = 'cloud'
    } else if (provider) {
      const model = VoiceSettings.model(provider)
      const voice = VoiceSettings.voice(provider)
      next =
        provider === 'openai'
          ? new OpenAIRealtimeEngine(model, voice, call)
          : new GeminiLiveEngine(model, voice, call)
      cloudStartedAt.current = Date.now()
      engineName.current = provider
    } else {
      next = new OnDeviceEngine(send)
      void call.start('device').catch(() => {})
      engineName.current = 'on_device'
    }
    quality.current!.begin(engineName.current)
    if (next instanceof GeminiLiveEngine) next.onStall = () => quality.current!.stalled()
    // Picks up the computer's newest models for the next call; this one starts right away.
    if (provider) void rememberStatus()
    // The computer decides what the voice hears (host chat/calls/announce); the engine only says it.
    const hear = (cue: CallCue) => next.hear(cue)
    next.onActivityChanged = (active) => {
      if (active) store.callCues.set(call.call, hear)
      else if (store.callCues.get(call.call) === hear) store.callCues.delete(call.call)
    }
    // A muted call stays muted on the engine that takes over.
    next.muted = engine.current?.muted === true
    engine.current = next
    setSession(next)
    void next.start()
  }

  /** After the call settings close: a different engine picked there takes over the call, which goes on. */
  const switchEngine = () => {
    const provider = VoiceSettings.provider()
    if (closed.current || VoiceSettings.engineChoice(provider) === engineChoice.current) return
    countCloudMinutes()
    engine.current?.end()
    setNotice(null)
    setBlocked(null)
    begin(provider)
  }

  const rememberStatus = async () => {
    try {
      return await voiceStatus(await store.ready())
    } catch {
      return null
    }
  }

  /**
   * No on-device speech here (Linux, or a build without the helper): use a provider with a key, if
   * any. On device stays the pick, so closing the settings unchanged keeps that provider.
   */
  const fallbackFromDevice = async () => {
    const picked = engineChoice.current
    const configured = (await rememberStatus())?.providers.find((p) => p.configured)?.provider
    // Ended, or another engine was picked meanwhile.
    if (closed.current || engineChoice.current !== picked) return
    if (configured) {
      setNotice(`${onDeviceUnavailableReason()} Using ${providerName(configured)}.`)
      if (window.codync.platform !== 'darwin') preferredEngine.set(configured)
      begin(configured)
      if (window.codync.platform === 'darwin') engineChoice.current = picked
    } else {
      setBlocked(`${onDeviceUnavailableReason()} Add an OpenAI or Gemini key in call settings.`)
      // Nothing runs: the next settings close tries again, with a key added there.
      engineChoice.current = ''
    }
  }

  useEffect(() => {
    const startedAt = Date.now()
    window.codync.speech.setCalling(true)
    begin(VoiceSettings.provider())
    return () => {
      window.codync.speech.setCalling(false)
      closed.current = true
      engine.current?.end()
      countCloudMinutes()
      onSpeaking(false)
      interrupt.current = null
      link.current?.end(Math.floor((Date.now() - startedAt) / 1000))
      quality.current?.report()
    }
  }, [])
  useEffect(() => voiceSettingsClosed.subscribe(switchEngine), [])

  const phase = session?.phase ?? null
  useEffect(() => {
    if (!session) return
    if (phase) quality.current!.phase(phase, session.failureKind)
    onSpeaking(phase === 'speaking')
    interrupt.current = phase === 'speaking' ? () => session.interrupt() : null
    // Once per call, so an engine taking over doesn't count or open it again.
    if (session.isActive && !counted.current) {
      counted.current = true
      analytics.track({ event: 'voice_call_started', engine: engineName.current })
      store.callStarted(botId, link.current!.call)
    }
    // A cloud engine that can't start or drops mid-call hands over to on-device voice.
    if (phase === 'failed' && session.provider && onDeviceAvailable()) {
      countCloudMinutes()
      setNotice(`${providerName(session.provider)} isn't available (${session.failure}). Using on-device voice.`)
      begin(null)
    }
  }, [session, phase])

  const speaking = phase === 'speaking'
  const muted = session?.muted === true
  const working = bot ? isWorking(bot) : false
  const connecting = session?.isConnecting ?? true
  const failure = blocked ?? (phase === 'failed' ? session?.failure : null)

  return (
    <div className="call">
      <div className="call-bar">
        <button
          className={`call-avatar ${speaking ? 'speaking' : ''}`}
          disabled={!speaking}
          aria-label={speaking ? 'Interrupt' : (bot?.name ?? '')}
          title={speaking ? 'Interrupt' : undefined}
          onClick={() => session?.interrupt()}
        >
          {bot ? <BotAvatar bot={bot} size={34} /> : <span className="call-avatar-empty" />}
        </button>
        <LevelDots
          sample={() => ({ level: session?.level ?? 0, speaking: session?.phase === 'speaking', working, connecting: connecting && !blocked })}
          color={color}
          width={120}
          label={statusText(phase, muted, working)}
          className="call-dots"
        />
        {/* Audio leaves the computer: say where. */}
        {session?.provider ? (
          <Text size="caption2" weight="semibold" tone="secondary" aria-label={`Voice by ${providerName(session.provider)}`}>
            {providerName(session.provider)}
          </Text>
        ) : null}
        <button className="call-round" title="Call settings" aria-label="Call settings" onClick={() => (openVoiceSettings ? openVoiceSettings() : setSettings(true))}>
          <Icon name="gearshape" size={16} weight="medium" scaled={false} />
        </button>
        <button
          className={`call-round ${muted ? 'on' : ''}`}
          title={muted ? 'Unmute' : 'Mute'}
          aria-label={muted ? 'Unmute' : 'Mute'}
          onClick={() => session && (session.muted = !session.muted)}
        >
          <Icon name={muted ? 'mic.slash' : 'mic'} size={16} weight="medium" scaled={false} />
        </button>
        <button className="call-round end" title="End call" aria-label="End call" onClick={onEnd}>
          <Icon name="xmark" size={16} weight="semibold" scaled={false} />
        </button>
      </div>
      {failure ? (
        <Text as="div" key="failure" tone="danger" className="call-line">
          {failure}
        </Text>
      ) : notice ? (
        <Text as="div" key={notice} tone="secondary" className="call-line">
          {notice}
        </Text>
      ) : null}
      <Sheet open={settings} onClose={() => setSettings(false)}>
        <VoiceChatSettingsView />
      </Sheet>
    </div>
  )
}
