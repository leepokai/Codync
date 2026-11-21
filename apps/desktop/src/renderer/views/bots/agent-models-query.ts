/**
 * Pure request & cache-scope helpers for `useAgentModels`, kept separate from React/store imports
 * so Node unit tests can exercise them directly.
 */
export function normalizeLoginProfile(loginProfile?: string | null): string | null {
  return loginProfile || null
}

export function agentModelsScopeKey(computerId: string, backend: string, loginProfile?: string | null): string {
  return `${computerId}/${backend}/${normalizeLoginProfile(loginProfile) ?? ''}`
}

export function agentModelsRequest(
  backend: string,
  loginProfile?: string | null,
): { backend: string; loginProfile?: string } {
  const profile = normalizeLoginProfile(loginProfile)
  return profile ? { backend, loginProfile: profile } : { backend }
}
