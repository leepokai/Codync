// App-level events the desktop app reports through the host's loopback `track`. The host accepts
// only the catalog's events and properties (docs/reference/analytics-events.md): feature use,
// never content.

/** Screens, sheets and tabs, named the same on every client. */
export type Screen =
  | 'chat'
  | 'trace'
  | 'bot_details'
  | 'memory'
  | 'routines'
  | 'marketplace'
  | 'settings'
  | 'computers'
  | 'usage'
  | 'account'
  | 'pairing'
  | 'voice_settings'
  | 'computer_access'
  | 'library'
  | 'artifact'

/** Outbound links, by what they lead to. */
export type Link = 'github' | 'docs' | 'issues' | 'discord' | 'app_store' | 'demo' | 'tailscale' | 'privacy' | 'support' | 'install_command'

export type SignInProvider = 'apple' | 'google'

/** Why a sign-in ended without an account. */
export type SignInFailure = 'cancelled' | 'timeout' | 'verification' | 'error'

/** What carried a voice call: on-device speech, a realtime model, or cloud transcription and speech. */
export type VoiceCallEngine = 'on_device' | 'openai' | 'gemini' | 'cloud'
/** The first phase of a call's engine that lasted too long. */
export type VoiceStuck = 'none' | 'connecting' | 'reconnecting' | 'speaking'
/** Why a call's engine gave up. */
export type VoiceFailure = 'none' | 'microphone' | 'connection' | 'provider'

export type AnalyticsEvent =
  | { event: 'app_opened'; at_login: boolean }
  | { event: 'window_opened' }
  | { event: 'onboarding_completed'; signed_in: boolean }
  | { event: 'computer_access_finished'; ready: boolean; onboarding: boolean }
  | { event: 'sign_in_started'; provider: SignInProvider }
  | { event: 'sign_in_failed'; provider: SignInProvider; reason: SignInFailure }
  | { event: 'signed_in'; provider?: SignInProvider }
  | { event: 'signed_out' }
  | { event: 'star_prompt_answered'; ask: 'first' | 'second'; starred: boolean }
  | { event: 'link_opened'; link: Link }
  | { event: 'screen_viewed'; screen: Screen }
  | { event: 'voice_call_started'; engine: VoiceCallEngine }
  | { event: 'voice_call_quality'; engine: VoiceCallEngine; ready_ms?: number; reconnects: number; stuck: VoiceStuck; failure: VoiceFailure }

/** `track`'s body: the event with its properties, from the desktop app. */
export const trackBody = (event: AnalyticsEvent) => ({ ...event, app: 'desktop' as const })

/**
 * Whether the app may send `host_setup_failed` itself (`host` is the owner's analytics setting as
 * its host last answered it): when it's on, or still unknown on a computer whose host never stored
 * one, so an owner who turned analytics off is never reported.
 */
export const maySendSetupFailure = (host: unknown, hostHasData: boolean) => host === true || (host === undefined && !hostHasData)
