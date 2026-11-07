/** The GitHub token of this computer's host, as `githubStatus` reports it, and its shape rules. */

export interface GitHubStatus {
  configured: boolean
  login: string | null
  scopes: string[]
}

export const githubTokenOk = (token: string) =>
  /^[A-Za-z0-9_-]{20,255}$/.test(token.trim())
