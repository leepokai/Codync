import { useEffect, useRef, useState } from 'react'
import type { HostClient } from '../../client/host-client'
import { useModel } from '../../lib/observable'
import { useStoreRef } from '../../store/context'
import { countVoiceUsage, micPCM16k, onDeviceUnavailableReason, providerName, transcribeOnHost, VoiceSettings, wav } from '../call/voice'
import { configured, voiceStatus } from '../settings/voice-settings'
import { Dictation, type DictationPlatform } from './dictation'

/**
 * Dictation's real surroundings: the speech helper and the microphone, and with `host` a provider
 * on the user's key through it; `report` shows its errors (the island has neither).
 */
export function dictationPlatform(host: (() => Promise<HostClient>) | null = null, report: (message: string) => void = () => {}): DictationPlatform {
  const client = async () => {
    if (!host) throw new Error('Dictating with a provider needs the chat window.')
    return host()
  }
  return {
    speech: window.codync.speech.available ? window.codync.speech : null,
    noSpeech: onDeviceUnavailableReason(),
    host: host !== null,
    language: () => VoiceSettings.language() || null,
    provider: VoiceSettings.dictationProvider,
    async checkKey(provider) {
      if (!configured(await voiceStatus(await client()), provider)) {
        throw new Error(`Add your ${providerName(provider)} key in Voice chat settings to dictate with it.`)
      }
    },
    microphone: micPCM16k,
    transcribe: async (provider, piece) => transcribeOnHost(await client(), provider, VoiceSettings.transcribeModel(provider), wav([piece])),
    count: countVoiceUsage,
    report,
  }
}

/** A composer's dictation; it stops when the composer goes away (another chat), and its errors show in the window. */
export function useDictation(editDraft: (change: (box: string) => string) => void) {
  const store = useStoreRef()
  const edit = useRef(editDraft)
  edit.current = editDraft
  const [dictation] = useState(
    () => new Dictation((change) => edit.current(change), dictationPlatform(() => store.ready(), (message) => store.setError(message))),
  )
  useEffect(() => () => dictation.cancel(), [dictation])
  return useModel(dictation)
}
