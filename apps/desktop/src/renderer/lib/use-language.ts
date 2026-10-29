import { useSyncExternalStore } from 'react'
import { prefs } from './prefs'
import { resetLine, translate, type AppLanguage } from './l10n'

export function useAppLanguage(): AppLanguage {
  return useSyncExternalStore(prefs.appLanguage.subscribe, prefs.appLanguage.get)
}

/** Translates one chrome string for the language chosen in Settings. */
export function useTr(): (text: string) => string {
  const language = useAppLanguage()
  const system = typeof navigator === 'undefined' ? 'en' : navigator.language
  return (text) => translate(text, language, system)
}

/** "resets 3h 20m" for the language chosen in Settings. */
export function useResetLine(): (raw: string) => string {
  const language = useAppLanguage()
  const system = typeof navigator === 'undefined' ? 'en' : navigator.language
  return (raw) => resetLine(raw, language, system)
}
