/**
 * Pure request & cache-scope helpers for `useAgentModels`, kept separate from React/store imports
 * so Node unit tests can exercise them directly. `profile` is already normalized: `null` is the default login.
 */
export function normalizeLoginProfile(loginProfile?: string | null): string | null {
  return loginProfile || null
}

export function agentModelsScopeKey(computerId: string, backend: string, profile: string | null): string {
  return `${computerId}/${backend}/${profile ?? ''}`
}

export function agentModelsRequest(backend: string, profile: string | null): { backend: string; loginProfile?: string } {
  return profile ? { backend, loginProfile: profile } : { backend }
}
