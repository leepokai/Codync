/**
 * Pure request helpers for `useAgentModels`, kept separate from React/store imports
 * so Node unit tests can exercise them directly. `profile` is already normalized: `null` is the default login.
 */
export function normalizeLoginProfile(loginProfile?: string | null): string | null {
  return loginProfile || null
}

/** `refresh` asks the computer to read the list from the agent again instead of answering with the one it kept. */
export function agentModelsRequest(backend: string, profile: string | null, refresh = false): { backend: string; loginProfile?: string; refresh?: true } {
  return { backend, ...(profile ? { loginProfile: profile } : {}), ...(refresh ? { refresh: true as const } : {}) }
}
