/** Desktop on-device recognition is supplied by the macOS speech helper. */
export function voiceEngine(value: string, platform: string): string {
  return platform !== 'darwin' && value === 'device' ? 'openai' : value
}

export function voiceEngineIds(platform: string): string[] {
  return [...(platform === 'darwin' ? ['device'] : []), 'openai', 'gemini']
}
