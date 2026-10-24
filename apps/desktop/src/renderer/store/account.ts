import type { SignInProvider } from '@shared/analytics'
import { Observable, useModel } from '../lib/observable'
import { analytics } from './analytics'

export interface AccountInfo {
  userId: string
  email: string | null
  avatarURL: string | null
}

/**
 * The Codync account (Clerk, through the main process). An account never grants access to a
 * computer by itself: each one still approves this device (spec §4.2).
 */
export class AccountSession extends Observable {
  isBusy = false
  isReady = true
  isConfigured = false
  errorMessage: string | null = null
  user: AccountInfo | null = null
  cloudURL: string | null = null
  /** The last sign-in started here, for `signed_in`. */
  provider: SignInProvider | null = null

  get isSignedIn() {
    return this.user !== null
  }

  async start() {
    const state = await window.codync.account.state()
    this.apply(state)
    window.codync.account.onChange((s) => this.apply(s))
  }

  private apply(s: Awaited<ReturnType<typeof window.codync.account.state>>) {
    this.isReady = s.ready
    this.isConfigured = s.configured
    this.isBusy = s.busy
    this.errorMessage = s.error
    this.user = s.user
    this.cloudURL = s.cloudURL
    this.changed()
  }

  async signIn(provider: SignInProvider) {
    this.provider = provider
    analytics.track({ event: 'sign_in_started', provider })
    const reason = await window.codync.account.signIn(provider)
    if (reason) analytics.track({ event: 'sign_in_failed', provider, reason })
  }

  async signOut() {
    await window.codync.account.signOut()
  }

  sessionToken() {
    return window.codync.account.token()
  }
}

export const account = new AccountSession()

export function useAccount() {
  return useModel(account)
}
