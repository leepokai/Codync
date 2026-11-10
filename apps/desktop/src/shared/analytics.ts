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

export type SignInProvider = 'apple' | 'google' | 'github'

/** Why a sign-in ended without an account. */
export type SignInFailure = 'cancelled' | 'timeout' | 'verification' | 'error'

/** What carried a voice call: on-device speech, a realtime model, or cloud transcription and speech. */
export type VoiceCallEngine = 'on_device' | 'openai' | 'gemini' | 'cloud'
/** The first phase of a call's engine that lasted too long. */
export type VoiceStuck = 'none' | 'connecting' | 'reconnecting' | 'speaking'
/** Why a call's engine gave up. */
export type VoiceFailure = 'none' | 'microphone' | 'connection' | 'provider'

/** What came of a window whose page crashed: it loaded again, or crashed too often and stays down. */
export type WindowRecoveryOutcome = 'reloaded' | 'gave_up'
/** How the ask before Reach from anywhere turns on ended. */
export type ReachOutcome = 'added' | 'own_relay' | 'cancelled' | 'failed'
/** Why a computer couldn't be added to the account: the cloud couldn't be reached, it refused, or the host failed. */
export type ClaimFailure = 'unreachable' | 'refused' | 'host'

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
  | { event: 'dev_server_action'; action: 'open' | 'copy' }
  | { event: 'screen_viewed'; screen: Screen }
  | { event: 'voice_call_started'; engine: VoiceCallEngine }
  | { event: 'voice_call_quality'; engine: VoiceCallEngine; ready_ms?: number; reconnects: number; stuck: VoiceStuck; failure: VoiceFailure }
  | { event: 'window_recovery'; outcome: WindowRecoveryOutcome }
  | { event: 'reach_account_answered'; outcome: ReachOutcome; error?: ClaimFailure }
  | { event: 'continued_without_account' }
  | { event: 'join_offer_answered'; outcome: 'added' | 'kept_out' | 'failed' }
  | { event: 'cloud_key_saved'; outcome: 'saved' | 'refused' | 'failed'; provider: string }
  | { event: 'cloud_computer_created'; outcome: 'created' | 'refused' | 'failed'; provider: string }
  | { event: 'cloud_offer_opened'; provider: string }
  | { event: 'cloud_computer_renamed'; outcome: 'renamed' | 'failed' }
  | { event: 'cloud_computer_deleted'; outcome: 'deleted' | 'refused' | 'failed'; provider: string }
  | { event: 'cloud_snapshots_deleted'; outcome: 'deleted' | 'refused' | 'failed'; provider: string }
  | { event: 'cloud_key_removed'; outcome: 'removed' | 'refused' | 'failed'; provider: string }
  | { event: 'kofi_invite_shown' }
  | { event: 'kofi_invite_answered'; opened: boolean }
  | { event: 'whats_new_shown' }

/** `track`'s body: the event with its properties, from the desktop app. */
export const trackBody = (event: AnalyticsEvent) => ({ ...event, app: 'desktop' as const })

/**
 * Whether the app may send `host_setup_failed` itself (`host` is the owner's analytics setting as
 * its host last answered it): when it's on, or still unknown on a computer whose host never stored
 * one, so an owner who turned analytics off is never reported.
 */
export const maySendSetupFailure = (host: unknown, hostHasData: boolean) => host === true || (host === undefined && !hostHasData)
