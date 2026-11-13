/** Desktop on-device recognition is supplied by the macOS speech helper. */
export function voiceEngine(value: string, platform: string): string {
  return platform !== 'darwin' && value === 'device' ? 'openai' : value
}

export function voiceEngineIds(platform: string): string[] {
  return [...(platform === 'darwin' ? ['device'] : []), 'openai', 'gemini']
}

/**
 * What the message box can dictate with: the call engines plus `local`, the computer's own
 * speech model (`voiceTranscribe` with provider `local`, no key). Dictation only: nothing
 * iterating `voiceEngineIds` (calls, fallbacks) ever offers it.
 */
export function dictationEngineIds(platform: string): string[] {
  return [...voiceEngineIds(platform), 'local']
}
