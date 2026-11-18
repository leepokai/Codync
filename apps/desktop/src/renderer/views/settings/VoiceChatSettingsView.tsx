import { onDeviceUnavailableReason, voiceSettingsClosed } from '../call/voice'
import { useEffect, useMemo, useRef, useState } from 'react'
import { LoopbackTransport, type HostClient } from '../../client/host-client'
import { Button, CardForm, CardSection, ChoicePicker, IconButton, Reveal, Spinner, Switch, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { usePref } from '../../lib/prefs'
import { useStore } from '../../store/context'
import { useScreenViewed } from '../../store/analytics'
import { SavedField } from './CommitField'
import { errorText, liveClient } from './parts'
import * as voice from './voice-settings'
import type { VoiceModels, VoiceProvider } from './voice-settings'
import { useTr } from '../../lib/use-language'
import { fill } from '@shared/fill'


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
  const [server] = usePref(voice.server(provider ?? 'openai'))
  const speechOnly = provider !== null && voice.speechOnly(server)
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
                  options={voice.engineOptions()}
                  onChange={setEngine}
                />
              </ValueRow>,
              ...(provider
                ? [
                    speechOnly
                      ? <ValueRow key="mode" label="Mode" detail="Your own server works in Speech mode only.">{tr('Speech')}</ValueRow>
                      : <ValueRow key="mode" label="Mode" detail={mode === 'speech' ? 'Sent when you pause, replies read aloud.' : 'A live conversation. Talk over replies.'}>
                          <ChoicePicker selection={mode} options={[{ id: 'realtime', label: 'Realtime' }, { id: 'speech', label: 'Speech' }]} onChange={setMode} />
                        </ValueRow>,
                    ...(speechOnly || mode === 'speech' ? [<PauseRow key="pause" />] : []),
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

/** The server the engine's Speech mode goes to: typed on the computer itself (`save`), read anywhere else. */
function ServerRow({ server, disabled, save }: { server: string; disabled: boolean; save?: (typed: string) => Promise<boolean> }) {
  const tr = useTr()
  if (!save) return <ValueRow label="Server">{voice.serverLabel(server)}</ValueRow>
  return (
    <VStack gap={4}>
      <SavedField label="Server" placeholder={voice.serverLabel('')} saved={server} disabled={disabled} save={save} />
      <Text size="footnote" tone="secondary" className="settings-row">{tr('An OpenAI-compatible server for Speech mode: https, or http on this computer. Empty uses OpenAI and removes the saved key.')}</Text>
    </VStack>
  )
}

const NO_MODELS: VoiceModels = { models: [], transcribe: [], speech: [] }

/**
 * One provider: the key the computer keeps, the server its Speech mode goes to, models, voice and this month's
 * minutes. `dictationOnly` keeps the key, the server and the speech-to-text model.
 */
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
  const [server] = usePref(voice.server(provider))
  const [replacing, setReplacing] = useState(false)
  const [key, setKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null)
  const [chosenVoice, setChosenVoice] = useState(() => voice.chosenVoice(provider))
  const [model, setModel] = useState(() => voice.chosenModel(provider, 'realtime'))
  const [transcribeModel, setTranscribeModel] = useState(() => voice.chosenModel(provider, 'transcribe'))
  const [speechModel, setSpeechModel] = useState(() => voice.chosenModel(provider, 'speech'))
  const [models, setModels] = useState<VoiceModels>(() => voice.modelsCache(provider).get() ?? NO_MODELS)
  const busyRef = useRef(false)
  const speechMode = voice.speechOnly(server) || mode === 'speech'
  /** Speech mode and dictation with this provider both use its speech-to-text model. */
  const transcribes = dictationOnly || speechMode || dictationEngine === provider
  const hasKey = saved === true
  const canEditKey = store.client?.transport instanceof LoopbackTransport
  const notConnected = { ok: false, text: tr('Not connected to {name}.').replaceAll('{name}', () => store.hostName) }

  /** `list` in the pickers, each with this device's choice. */
  const showModels = (list: VoiceModels) => {
    setModels(list)
    setModel(voice.chosenModel(provider, 'realtime'))
    setTranscribeModel(voice.chosenModel(provider, 'transcribe'))
    setSpeechModel(voice.chosenModel(provider, 'speech'))
  }

  const loadModels = async (client: HostClient) => {
    let list: VoiceModels
    try {
      list = await voice.voiceModels(client, provider)
    } catch {
      return
    }
    // A newer default moves every choice the user hasn't made themselves.
    voice.rememberDefaults(provider, list.defaults)
    showModels(list)
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

  /** Whether `work` finished; a failure shows under the key. */
  const run = async (work: (client: HostClient) => Promise<void>): Promise<boolean> => {
    if (busyRef.current) return false
    const client = liveClient(store)
    if (!client) {
      setResult(notConnected)
      return false
    }
    busyRef.current = true
    setBusy(true)
    setResult(null)
    try {
      await work(client)
      return true
    } catch (e) {
      setResult({ ok: false, text: errorText(e) })
      return false
    } finally {
      busyRef.current = false
      setBusy(false)
    }
  }

  /**
   * A pasted key is checked with the provider by the computer before it's kept. With a key saved, Test starts a call
   * credential with the chosen model and voice; for dictation only, it lists the key's models.
   */
  const test = () => {
    const typed = key.trim()
    if (!typed && !hasKey) return
    void run(async (client) => {
      if (!typed || (!replacing && hasKey)) {
        if (dictationOnly) await voice.voiceModels(client, provider)
        else if (speechMode) await voice.voiceSpeak(client, provider, speechModel, chosenVoice, 'OK')
        else await voice.voiceSession(client, provider, model, chosenVoice)
        setResult({ ok: true, text: tr('Key works') })
      } else {
        if (!(client.transport instanceof LoopbackTransport)) return
        setSaved(voice.configured(await voice.setVoiceKey(client, provider, typed), provider))
        setKey('')
        setReplacing(false)
        setResult({ ok: true, text: tr('Key works and is saved on {name}').replaceAll('{name}', () => store.hostName) })
      }
      await loadModels(client)
    })
  }

  const remove = () =>
    void run(async (client) => {
      if (!(client.transport instanceof LoopbackTransport)) return
      setSaved(voice.configured(await voice.setVoiceKey(client, provider, ''), provider))
      setResult(null)
    })

  /**
   * The computer checks a saved key with the new server before keeping the address; a refused one leaves the old.
   * Clearing it also removes the key. Another server has other models: the picks made for the old one are gone.
   */
  const saveServer = (typed: string) =>
    run(async (client) => {
      if (!(client.transport instanceof LoopbackTransport)) return
      const configured = voice.configured(await voice.setVoiceServer(client, provider, typed), provider)
      setSaved(configured)
      showModels(voice.modelsCache(provider).get() ?? NO_MODELS)
      setResult({ ok: true, text: tr('Saved') })
      if (configured) await loadModels(client)
    })

  const picker = (selection: string, list: string[], onChange: (m: string) => void) => {
    // The current choice stays listed before the provider's list arrives.
    const options = list.includes(selection) ? list : [selection, ...list]
    return <ChoicePicker selection={selection} options={options.map((m) => ({ id: m, label: m }))} onChange={onChange} />
  }

  const dimmed = hasKey ? 'settings-dimmable' : 'settings-dimmable dimmed'
  let footer: string | null = null
  if (!hasKey) footer = fill(tr("Add a key to choose from {name}'s models."), { name: info.name })
  else if (dictationOnly) footer = tr('For dictation in the message box.')
  const keyArea = (
    <VStack key="key" gap={8}>
      {hasKey && !replacing ? (
        // The key is on the computer: say so, and offer test / replace / remove.
        <div className="settings-row">
          <Icon name="key.fill" size={13} color="var(--secondary)" />
          <VStack grow className="settings-stack">
            <Text tone="text" truncate>{tr(`Key saved`)}</Text>
            <Text size="caption" tone="tertiary" truncate>{tr('Stays on {name}').replaceAll('{name}', () => store.hostName)}</Text>
          </VStack>
          {busy ? (
            <Spinner />
          ) : (
            <>
              <IconButton title="Test key" icon="checkmark.seal" onClick={test} />
              {canEditKey ? <>
                <IconButton title="Replace key" icon="pencil" onClick={() => { setReplacing(true); setResult(null) }} />
                <IconButton title="Remove key" icon="trash" onClick={remove} />
              </> : null}
            </>
          )}
        </div>
      ) : !canEditKey ? (
        <VStack gap={4} className="settings-row">
          <Text>{fill(tr('No {name} key on {other}'), { name: info.name, other: store.hostName })}</Text>
          <Text size="footnote" tone="secondary">{tr('Add it in Codync on {name}: Settings → Voice chat, or connect to that computer over SSH.').replaceAll('{name}', () => store.hostName)}</Text>
        </VStack>
      ) : (
        // Paste a key; the computer checks it with the provider before keeping it.
        <VStack gap={10}>
          <HStack gap={8} className="settings-row">
            <input
              type="password"
              aria-label={tr('API key')}
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
              {tr('Get a key from {name}').replaceAll('{name}', () => info.name)}
            </button>
            <Spacer />
            {replacing ? (
              <button className="settings-quiet-button" onClick={() => { setReplacing(false); setKey(''); setResult(null) }}>
                {tr('Cancel')}
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
        ...(server === null ? [] : [<ServerRow key="server" server={server} disabled={busy} save={canEditKey ? saveServer : undefined} />]),
        ...(transcribes
          ? [<div key="stt" className={dimmed}><ValueRow label={tr('Speech to text')}>{picker(transcribeModel, models.transcribe, pick('transcribe', setTranscribeModel))}</ValueRow></div>]
          : []),
        ...(dictationOnly
          ? []
          : [
              speechMode
                ? <div key="tts" className={dimmed}><ValueRow label={tr('Text to speech')}>{picker(speechModel, models.speech, pick('speech', setSpeechModel))}</ValueRow></div>
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
                <ValueRow label={tr('This month')}>{tr('≈ {minutes} min').replaceAll('{minutes}', () => String(voice.voiceMinutes(provider)))}</ValueRow>
              </div>,
            ]),
      ]}
    </CardSection>
  )
}
