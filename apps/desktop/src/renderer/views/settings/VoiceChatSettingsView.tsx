import { onDeviceUnavailableReason, voiceSettingsClosed } from '../call/voice'
import { useEffect, useMemo, useRef, useState } from 'react'
import type { HostClient } from '../../client/host-client'
import { Button, CardForm, CardSection, ChoicePicker, IconButton, Reveal, Spinner, Switch, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { usePref } from '../../lib/prefs'
import { useStore } from '../../store/context'
import { useScreenViewed } from '../../store/analytics'
import { errorText, liveClient } from './parts'
import * as voice from './voice-settings'
import type { VoiceModels, VoiceProvider } from './voice-settings'
import { useTr } from '../../lib/use-language'


/**
 * Voice chat settings: which voice carries calls (on-device speech, or OpenAI / Gemini on the
 * user's own key kept by the computer), the mode, models and voice. The computer is the store
 * in `StoreContext`. `showsHeader` is false inside Settings, which titles the page itself.
 */
export function VoiceChatSettingsView({ showsHeader = true }: { showsHeader?: boolean }) {
  const tr = useTr()
  useScreenViewed('voice_settings')
  const [engine, setEngine] = usePref(voice.engine)
  const [rate, setRate] = usePref(voice.rate)
  const [language, setLanguage] = usePref(voice.language)
  const languages = useMemo(() => voice.languageOptions(window.codync.speech.locales()), [])
  const provider = voice.isVoiceProvider(engine) ? engine : null
  const [dictationEngine] = usePref(voice.dictationEngine)
  /** Dictation on another provider than calls still needs that provider's key and speech-to-text model. */
  const dictationProvider = voice.isVoiceProvider(dictationEngine) && dictationEngine !== provider ? dictationEngine : null
  const [mode, setMode] = usePref(voice.mode(provider ?? 'openai'))
  // Closing hands a running call to a different engine picked here.
  useEffect(() => () => voiceSettingsClosed.ring(), [])
  const footer = provider
    ? `Your own ${voice.voiceInfo[provider].name} key, billed by ${voice.voiceInfo[provider].name}. During a call, applies when you close settings.`
    : window.codync.speech.available
      ? 'On-device speech, on this computer. During a call, applies when you close settings.'
      : onDeviceUnavailableReason()
  return (
    <SheetPage className={showsHeader ? 'settings-voice-sheet' : undefined}>
      {showsHeader ? <ModalHeader icon="waveform" title={tr(`Voice chat`)} /> : null}
      <div className="settings-voice-body">
        <CardForm>
          <CardSection title="Voice" footer={footer}>
            {[
              <ValueRow key="engine" label="Engine">
                <ChoicePicker
                  selection={engine}
                  options={[...(window.codync.speech.available || engine === 'device' ? [{ id: 'device', label: 'On device' }] : []), ...voice.VOICE_PROVIDERS.map((p) => ({ id: p, label: voice.voiceInfo[p].name }))]}
                  onChange={setEngine}
                />
              </ValueRow>,
              ...(provider
                ? [
                    <ValueRow key="mode" label="Mode" detail={mode === 'speech' ? 'Sent when you pause, replies read aloud.' : 'A live conversation. Talk over replies.'}>
                      <ChoicePicker selection={mode} options={[{ id: 'realtime', label: 'Realtime' }, { id: 'speech', label: 'Speech' }]} onChange={setMode} />
                    </ValueRow>,
                    ...(mode === 'speech' ? [<PauseRow key="pause" />] : []),
                  ]
                : [
                    <ValueRow key="language" label="Language" detail="The language you speak in calls.">
                      <ChoicePicker selection={language} options={languages} onChange={setLanguage} />
                    </ValueRow>,
                    <ValueRow key="rate" label="Reading speed">
                      <ChoicePicker
                        selection={rate}
                        options={[{ id: 0.42, label: 'Slower' }, { id: voice.DEFAULT_RATE, label: 'Normal' }, { id: 0.56, label: 'Faster' }]}
                        onChange={setRate}
                      />
                    </ValueRow>,
                    <PauseRow key="pause" />,
                  ]),
            ]}
          </CardSection>
          {provider ? <ProviderVoiceSettings key={provider} provider={provider} /> : null}
          {dictationProvider ? <ProviderVoiceSettings key={`dictation-${dictationProvider}`} provider={dictationProvider} dictationOnly /> : null}
        </CardForm>
      </div>
    </SheetPage>
  )
}

/** How long a silence ends what you say, before it's sent. */
function PauseRow() {
  const [pause, setPause] = usePref(voice.pause)
  return (
    <div className="settings-appear">
      <ValueRow label="Send after a pause of" detail="Longer leaves time to think mid-sentence.">
      <ChoicePicker selection={pause} options={[{ id: 1, label: '1 s' }, { id: 1.5, label: '1.5 s' }, { id: 2.5, label: '2.5 s' }]} onChange={setPause} />
      </ValueRow>
    </div>
  )
}

/** One provider: the key the computer keeps, models, voice and this month's minutes. `dictationOnly` keeps the key and speech-to-text model. */
function ProviderVoiceSettings({ provider, dictationOnly = false }: { provider: VoiceProvider; dictationOnly?: boolean }) {
  const tr = useTr()
  const store = useStore()
  const info = voice.voiceInfo[provider]
  const [googleSearch, setGoogleSearch] = usePref(voice.googleSearch)
  const [supportsSearch, setSupportsSearch] = useState(false)
  const [mode] = usePref(voice.mode(provider))
  const [dictationEngine] = usePref(voice.dictationEngine)
  /** null while the computer hasn't answered yet. */
  const [saved, setSaved] = useState<boolean | null>(null)
  const [replacing, setReplacing] = useState(false)
  const [key, setKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null)
  const [chosenVoice, setChosenVoice] = useState(() => voice.chosenVoice(provider))
  const [model, setModel] = useState(() => voice.chosenModel(provider, 'realtime'))
  const [transcribeModel, setTranscribeModel] = useState(() => voice.chosenModel(provider, 'transcribe'))
  const [speechModel, setSpeechModel] = useState(() => voice.chosenModel(provider, 'speech'))
  const [models, setModels] = useState<VoiceModels>(() => voice.modelsCache(provider).get() ?? { models: [], transcribe: [], speech: [] })
  const busyRef = useRef(false)
  const speechMode = mode === 'speech'
  /** Speech mode and dictation with this provider both use its speech-to-text model. */
  const transcribes = dictationOnly || speechMode || dictationEngine === provider
  const hasKey = saved === true
  const notConnected = { ok: false, text: `Not connected to ${store.hostName}.` }

  const loadModels = async (client: HostClient) => {
    let list: VoiceModels
    try {
      list = await voice.voiceModels(client, provider)
    } catch {
      return
    }
    setModels(list)
    // A newer default moves every choice the user hasn't made themselves.
    voice.rememberDefaults(provider, list.defaults)
    setModel(voice.chosenModel(provider, 'realtime'))
    setTranscribeModel(voice.chosenModel(provider, 'transcribe'))
    setSpeechModel(voice.chosenModel(provider, 'speech'))
    voice.modelsCache(provider).set(list)
  }

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const client = liveClient(store)
      if (!client) {
        setSaved(false)
        return setResult(notConnected)
      }
      let configured: boolean
      try {
        const status = await voice.voiceStatus(client)
        configured = voice.configured(status, provider)
        if (!cancelled) setSupportsSearch(status.googleSearch === true)
      } catch (e) {
        if (cancelled) return
        setSaved(false)
        return setResult({ ok: false, text: errorText(e) })
      }
      if (cancelled) return
      setSaved(configured)
      if (configured) await loadModels(client)
    })()
    return () => void (cancelled = true)
  }, [provider])

  const pick = (kind: voice.ModelKind, set: (m: string) => void) => (m: string) => {
    set(m)
    voice.saveModel(provider, kind, m)
  }

  const run = (work: (client: HostClient) => Promise<void>) => {
    if (busyRef.current) return
    const client = liveClient(store)
    if (!client) return setResult(notConnected)
    busyRef.current = true
    setBusy(true)
    setResult(null)
    void work(client)
      .catch((e: unknown) => setResult({ ok: false, text: errorText(e) }))
      .finally(() => {
        busyRef.current = false
        setBusy(false)
      })
  }

  /**
   * A pasted key is checked with the provider by the computer before it's kept. With a key saved, Test starts a call
   * credential with the chosen model and voice; for dictation only, it lists the key's models.
   */
  const test = () => {
    const typed = key.trim()
    if (!typed && !hasKey) return
    run(async (client) => {
      if (!typed || (!replacing && hasKey)) {
        if (dictationOnly) await voice.voiceModels(client, provider)
        else if (speechMode) await voice.voiceSpeak(client, provider, speechModel, chosenVoice, 'OK')
        else await voice.voiceSession(client, provider, model, chosenVoice)
        setResult({ ok: true, text: 'Key works' })
      } else {
        setSaved(voice.configured(await voice.setVoiceKey(client, provider, typed), provider))
        setKey('')
        setReplacing(false)
        setResult({ ok: true, text: `Key works and is saved on ${store.hostName}` })
      }
      await loadModels(client)
    })
  }

  const remove = () =>
    run(async (client) => {
      setSaved(voice.configured(await voice.setVoiceKey(client, provider, ''), provider))
      setResult(null)
    })

  const picker = (selection: string, list: string[], onChange: (m: string) => void) => {
    // The current choice stays listed before the provider's list arrives.
    const options = list.includes(selection) ? list : [selection, ...list]
    return <ChoicePicker selection={selection} options={options.map((m) => ({ id: m, label: m }))} onChange={onChange} />
  }

  const dimmed = hasKey ? 'settings-dimmable' : 'settings-dimmable dimmed'
  let footer: string | null = null
  if (!hasKey) footer = `Add a key to choose from ${info.name}'s models.`
  else if (dictationOnly) footer = 'For dictation in the message box.'
  const keyArea = (
    <VStack key="key" gap={8}>
      {hasKey && !replacing ? (
        // The key is on the computer: say so, and offer test / replace / remove.
        <div className="settings-row">
          <Icon name="key.fill" size={13} color="var(--secondary)" />
          <VStack grow className="settings-stack">
            <Text tone="text" truncate>{tr(`Key saved`)}</Text>
            <Text size="caption" tone="tertiary" truncate>Stays on {store.hostName}</Text>
          </VStack>
          {busy ? (
            <Spinner />
          ) : (
            <>
              <IconButton title="Test key" icon="checkmark.seal" onClick={test} />
              <IconButton title="Replace key" icon="pencil" onClick={() => { setReplacing(true); setResult(null) }} />
              <IconButton title="Remove key" icon="trash" onClick={remove} />
            </>
          )}
        </div>
      ) : (
        // Paste a key; the computer checks it with the provider before keeping it.
        <VStack gap={10}>
          <HStack gap={8} className="settings-row">
            <input
              type="password"
              aria-label="API key"
              placeholder={info.keyPrompt}
              value={key}
              spellCheck={false}
              autoComplete="off"
              className="settings-key-input"
              onChange={(e) => setKey(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && test()}
            />
            {busy ? (
              <Spinner />
            ) : (
              <Button onClick={test} disabled={!key.trim()}>
                Save
              </Button>
            )}
          </HStack>
          <Text as="div" size="footnote" className="settings-row">
            <button className="settings-link" onClick={() => window.codync.app.openExternal(info.keyPage)}>
              Get a key from {info.name}
            </button>
            <Spacer />
            {replacing ? (
              <button className="settings-quiet-button" onClick={() => { setReplacing(false); setKey(''); setResult(null) }}>
                Cancel
              </button>
            ) : null}
          </Text>
        </VStack>
      )}
      <Reveal show={result !== null}>
        {result ? (
          <HStack as="span" gap={5} className={`settings-key-result ${result.ok ? '' : 'failed'}`}>
            <Icon name={result.ok ? 'checkmark.circle.fill' : 'exclamationmark.circle.fill'} size={10} />
            {result.text}
          </HStack>
        ) : null}
      </Reveal>
    </VStack>
  )

  return (
    <CardSection title={info.name} footer={footer}>
      {[
        keyArea,
        ...(transcribes
          ? [<div key="stt" className={dimmed}><ValueRow label="Speech to text">{picker(transcribeModel, models.transcribe, pick('transcribe', setTranscribeModel))}</ValueRow></div>]
          : []),
        ...(dictationOnly
          ? []
          : [
              speechMode
                ? <div key="tts" className={dimmed}><ValueRow label="Text to speech">{picker(speechModel, models.speech, pick('speech', setSpeechModel))}</ValueRow></div>
                : <div key="model" className={dimmed}><ValueRow label="Model">{picker(model, models.models, pick('realtime', setModel))}</ValueRow></div>,
              <div key="voice" className={dimmed}>
                <ValueRow label="Voice">
                  <ChoicePicker
                    selection={chosenVoice}
                    options={info.voices.map((v) => ({ id: v, label: v }))}
                    onChange={(v) => {
                      setChosenVoice(v)
                      voice.saveVoice(provider, v)
                    }}
                  />
                </ValueRow>
              </div>,
              ...(provider === 'gemini' && !speechMode && supportsSearch ? [
                <ValueRow key="google-search" label="Google Search" detail="Let Gemini search the web during calls. Requires Google Search quota; charges may apply.">
                  <Switch label="Google Search" on={googleSearch} onChange={setGoogleSearch} />
                </ValueRow>,
              ] : []),
              <div key="minutes" className={dimmed}>
                <ValueRow label="This month">≈ {voice.voiceMinutes(provider)} min</ValueRow>
              </div>,
            ]),
      ]}
    </CardSection>
  )
}
