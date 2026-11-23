// App-level events the desktop app reports through the host's loopback `track`. The host accepts
// only the catalog's events and properties (docs/reference/analytics-events.md): feature use,
// never content.

import type { AlertAnalyticsKind } from './alerts'

/** Screens, sheets and tabs, named the same on every client. */
export type Screen =
  | 'chat'
  | 'trace'
  | 'bot_details'
  | 'bot_computer'
  | 'bot_browser'
  | 'side_chat'
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
  | 'remote_screen'
  | 'thread'
  | 'all_work'
  | 'workflows'
  | 'workflow'
  | 'workflowRun'
  | 'workflowEditor'

/** Outbound links, by what they lead to. */
export type Link = 'github' | 'docs' | 'issues' | 'discord' | 'app_store' | 'demo' | 'tailscale' | 'privacy' | 'support' | 'install_command' | 'whats_new_film'

/** What was done in the full-size picture viewer. */
export type ImageAction = 'zoom' | 'copy_image' | 'copy_name' | 'download'
/** Where it was done: inside the full-size viewer, or from a picture's menu in a chat or the Library. */
export type ImageFrom = 'viewer' | 'menu'
/** Why a picture copy or download failed: its bytes weren't there, couldn't be converted, the clipboard refused them, or the transfer broke. */
export type ImageFailure = 'unavailable' | 'convert' | 'clipboard' | 'transfer'

export type SignInProvider = 'apple' | 'google' | 'github'

/** Why a sign-in ended without an account. */
export type SignInFailure = 'cancelled' | 'timeout' | 'verification' | 'error'

/** What carried a voice call: on-device speech, a realtime model, or cloud transcription and speech. */
export type VoiceCallEngine = 'on_device' | 'openai' | 'gemini' | 'cloud'
/** The first phase of a call's engine that lasted too long. */
export type VoiceStuck = 'none' | 'connecting' | 'reconnecting' | 'speaking'
/** Why a call's engine gave up. */
export type VoiceFailure = 'none' | 'microphone' | 'connection' | 'provider'
/** Why an access request to an account computer ended without access (the phones' reasons). */
export type AccessFailure = 'denied' | 'expired' | 'cancelled'

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
  | { event: 'voice_note_played'; outcome: 'played' }
  | { event: 'voice_note_played'; outcome: 'failed'; error: 'load' | 'playback' }
  | { event: 'screen_viewed'; screen: Screen }
  | { event: 'remote_screen_paused'; screen: 'bot_computer' | 'remote_screen' }
  | { event: 'image_preview_opened' }
  | { event: 'image_preview_action'; action: ImageAction; from: ImageFrom; outcome: 'done' | 'failed'; error?: ImageFailure }
  | { event: 'conversation_find_opened' }
  | { event: 'conversation_find_searched'; found: boolean }
  | { event: 'quote_opened'; found: boolean }
  | { event: 'voice_call_started'; engine: VoiceCallEngine }
  | { event: 'voice_call_quality'; engine: VoiceCallEngine; ready_ms?: number; reconnects: number; stuck: VoiceStuck; failure: VoiceFailure
      /** Gemini's Google Search was on; `searches`: answers with its suggestions (never the query). */
      google_search?: true; searches?: number }
  | { event: 'access_requested' }
  | { event: 'access_request_failed'; reason: AccessFailure }
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
  | { event: 'kofi_invite_shown'; ask: number }
  | { event: 'kofi_invite_answered'; ask: number; opened: boolean }
  | { event: 'whats_new_shown' }
  | { event: 'panel_split'; on: boolean }
  | { event: 'panel_maximized'; on: boolean }
  | { event: 'preview_opened'; from: 'address' | 'dev_server' }
  | { event: 'list_layout_changed'; list: 'bots' | 'threads'; option: 'folder' | 'bot' | 'computer' | 'status' | 'activity'; on: boolean }
  | { event: 'desktop_notifications_set'; on: boolean }
  | { event: 'desktop_notification_opened'; kind: AlertAnalyticsKind }

/** `track`'s body: the event with its properties, from the desktop app. */
export const trackBody = (event: AnalyticsEvent) => ({ ...event, app: 'desktop' as const })

/**
 * Whether the app may send `host_setup_failed` itself (`host` is the owner's analytics setting as
 * its host last answered it): when it's on, or still unknown on a computer whose host never stored
 * one, so an owner who turned analytics off is never reported.
 */
export const maySendSetupFailure = (host: unknown, hostHasData: boolean) => host === true || (host === undefined && !hostHasData)
