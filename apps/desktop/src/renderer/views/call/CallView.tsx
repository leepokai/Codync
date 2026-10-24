import type { VoiceCallEngine } from '@shared/analytics'
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
import { voiceStatus } from '../settings/voice-settings'
import { stopDictation } from '../thread/dictation'
import { CallOperator } from './call-operator'

/**
 * Opens the window's own Settings at Voice chat; without it (no Settings screen around) the
 * call bar shows the voice settings in their own sheet.
 */
export const OpenVoiceSettings = createContext<(() => void) | null>(null)
import { CloudSpeechEngine } from './CloudSpeechEngine'
import { GeminiLiveEngine } from './GeminiLiveEngine'
import { OnDeviceEngine } from './OnDeviceEngine'
import { OpenAIRealtimeEngine } from './OpenAIRealtimeEngine'
import { countVoiceUsage, fromBase64, onDeviceAvailable, onDeviceUnavailableReason, providerName, transcribeOnHost, VoiceEngine, VoiceSettings, type VoiceProvider } from './voice'
import { watchVoiceCall } from './voice-call'
import './call.css'

/**
 * A voice call with a bot, as Grok Bot does it: a floating bar over the chat, which stays
 * readable underneath. What you say goes out as ordinary messages, its final replies are read
 * aloud, and the chat gets a "Voice chat · 00:16" line when the call ends.
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
  const closed = useRef(false)
  /** What carries the current engine's audio, and whether the call has been counted (once its first engine runs). */
  const engineName = useRef<VoiceCallEngine>('on_device')
  const counted = useRef(false)
  const bot = store.bots.get(botId)
  const color = bot ? avatarColor(bot.avatarColor) : null

  const countCloudMinutes = () => {
    const provider = engine.current?.provider
    if (!provider || cloudStartedAt.current === null) return
    countVoiceUsage((Date.now() - cloudStartedAt.current) / 1000, provider)
    cloudStartedAt.current = null
  }

  const credential = (provider: VoiceProvider, model: string, voice: string) => async () => {
    const client = await store.ready()
    return (await client.call<{ credential: string }>('voiceSession', { provider, model, voice }, 30_000)).credential
  }

  /** Starts the engine: a realtime provider on the key kept by the computer, or on-device speech. */
  const begin = (provider: VoiceProvider | null) => {
    if (closed.current) return
    // Every engine takes the microphone (and the speech helper) from any dictation in progress.
    stopDictation()
    if (!provider && !onDeviceAvailable()) return void fallbackFromDevice()
    const op = new CallOperator(store, botId)
    const send = (text: string) => store.send(text, botId)
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
      cloudStartedAt.current = Date.now()
      engineName.current = 'cloud'
    } else if (provider) {
      const model = VoiceSettings.model(provider)
      const voice = VoiceSettings.voice(provider)
      next =
        provider === 'openai'
          ? new OpenAIRealtimeEngine(credential(provider, model, voice), op)
          : new GeminiLiveEngine(model, voice, credential(provider, model, voice), op)
      cloudStartedAt.current = Date.now()
      engineName.current = provider
    } else {
      next = new OnDeviceEngine(send)
      engineName.current = 'on_device'
    }
    // Picks up the computer's newest models for the next call; this one starts right away.
    if (provider) void rememberStatus()
    let stopWatching: (() => void) | null = null
    next.onActivityChanged = (active) => {
      if (active && !stopWatching) {
        stopWatching = watchVoiceCall(store, botId, { speak: (t) => next.speak(t), announce: (t) => next.announce(t) })
      } else if (!active && stopWatching) {
        stopWatching()
        stopWatching = null
      }
    }
    engine.current = next
    setSession(next)
    void next.start()
  }

  const rememberStatus = async () => {
    try {
      return await voiceStatus(await store.ready())
    } catch {
      return null
    }
  }

  /** No on-device speech here (Linux, or a build without the helper): use a provider with a key, if any. */
  const fallbackFromDevice = async () => {
    const configured = (await rememberStatus())?.providers.find((p) => p.configured)?.provider
    if (closed.current) return
    if (configured) {
      setNotice(`${onDeviceUnavailableReason()} Using ${providerName(configured)}.`)
      begin(configured)
    } else {
      setBlocked(`${onDeviceUnavailableReason()} Add an OpenAI or Gemini key in call settings.`)
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
      store.logCall(botId, Math.floor((Date.now() - startedAt) / 1000))
    }
  }, [])

  const phase = session?.phase ?? null
  useEffect(() => {
    if (!session) return
    onSpeaking(phase === 'speaking')
    interrupt.current = phase === 'speaking' ? () => session.interrupt() : null
    if (session.isActive && !counted.current) {
      counted.current = true
      analytics.track({ event: 'voice_call_started', engine: engineName.current })
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
  const statusText =
    phase === null || phase === 'starting'
      ? 'Connecting'
      : phase === 'speaking'
        ? 'Speaking'
        : phase === 'failed'
          ? "Can't listen"
          : muted
            ? 'Muted'
            : working
              ? 'Working, listening'
              : 'Listening'
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
          sample={() => ({ level: session?.level ?? 0, speaking: session?.phase === 'speaking', working })}
          color={color}
          width={120}
          label={statusText}
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
