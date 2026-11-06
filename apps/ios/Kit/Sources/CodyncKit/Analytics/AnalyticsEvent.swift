import Foundation

/// What the iPhone reports, each with its properties (docs/reference/analytics-events.md). Bot and
/// message events come from the computer's host instead. Every value is a count, a boolean or a
/// name from these enums: nothing typed by the user can get in.
public enum AnalyticsEvent: Equatable, Sendable {
    /// nil = couldn't be read; left out.
    case appOpened(notifications: Notifications? = nil, widgets: Int? = nil)
    case onboardingStepViewed(OnboardingStep)
    case onboardingCompleted(via: OnboardingVia)
    case signInStarted(SignInProvider)
    case signInFailed(SignInProvider, reason: SignInFailure)
    case signedIn(SignInProvider)
    case signedOut
    case pairingFailed(PairingFailure)
    case computerPaired(via: PairVia, computer: ComputerID)
    case accessRequested
    case accessRequestFailed(AccessFailure)
    case notificationsAnswered(granted: Bool, source: PromptSource)
    case notificationOpened(NotificationKind)
    case settingChanged(Setting, on: Bool)
    case linkOpened(Link)
    case voiceCallStarted(VoiceEngine)
    /// How one engine of a call fared, when it's replaced or the call ends: milliseconds until it was ready (nil:
    /// never), reconnects, the first phase stuck too long, why it failed.
    case voiceCallQuality(VoiceEngine, readyMs: Int?, reconnects: Int, stuck: VoiceStuck, failure: VoiceFailure)
    case screenViewed(Screen)
    case reviewOpportunity(completed: Int)
    case reviewCancelled(ReviewCancellation)
    case reviewRequested(completed: Int)
    case reviewLinkOpened

    public enum ReviewCancellation: String, Sendable { case background, interaction, unavailable }

    public enum Notifications: String, Sendable { case allowed, denied, notAsked = "not_asked" }
    public enum OnboardingStep: String, Sendable { case welcome, install, scan }
    public enum OnboardingVia: String, Sendable { case paired, account, skipped }
    public enum SignInProvider: String, Sendable { case apple, google }
    public enum SignInFailure: String, Sendable { case cancelled, verification, error }
    public enum PairVia: String, Sendable { case qr, link, account }
    public enum AccessFailure: String, Sendable { case denied, expired, cancelled }
    public enum PromptSource: String, Sendable { case settings, widgets }
    public enum Setting: String, Sendable { case push, liveActivities = "live_activities" }
    public enum Link: String, Sendable {
        case github, issues, appStore = "app_store", demo, tailscale, privacy, installCommand = "install_command"
    }
    public enum VoiceEngine: String, Sendable { case onDevice = "on_device", openai, gemini, cloud }
    public enum VoiceStuck: String, Sendable { case none, connecting, reconnecting, speaking }
    public enum VoiceFailure: String, Sendable { case none, microphone, connection, provider }
    public enum Screen: String, Sendable {
        case bots, state, chat, trace, botDetails = "bot_details", memory, routines, marketplace
        case settings, computers, usage, account, pairing, voiceSettings = "voice_settings", remoteScreen = "remote_screen"
        case library, artifact, network
    }

    public enum NotificationKind: String, Sendable {
        case done, needsInput = "needs_input", failed

        /// The alert's APNs category (the host's alert kind); nil for anything else.
        public init?(category: String) {
            switch category {
            case "done": self = .done
            case "needsInput": self = .needsInput
            case "failed": self = .failed
            default: return nil
            }
        }
    }

    public enum PairingFailure: String, Sendable {
        case notPairingLink = "not_pairing_link", outdatedHost = "outdated_host", invalid
        case expired, unauthorized, unreachable, timeout, upgradeRequired = "upgrade_required", other

        /// nil when pairing was called off rather than failed (the account changed meanwhile).
        public init?(_ error: any Error) {
            switch error {
            case let problem as Pairing.Problem:
                self = switch problem {
                case .notPairingLink: .notPairingLink
                case .outdatedHost: .outdatedHost
                case .invalid: .invalid
                }
            case let host as HostError:
                self = switch host {
                case .upgradeRequired: .upgradeRequired
                // The host turns down a pairing code that expired or was already used.
                case .http(403, _): .expired
                case .unauthorized, .http(401, _): .unauthorized
                case .unreachable, .computerOffline: .unreachable
                case .http: .other
                }
            case let url as URLError where url.code == .timedOut: self = .timeout
            case is CancellationError: return nil
            default: self = .other
            }
        }
    }

    var name: String {
        switch self {
        case .appOpened: "app_opened"
        case .onboardingStepViewed: "onboarding_step_viewed"
        case .onboardingCompleted: "onboarding_completed"
        case .signInStarted: "sign_in_started"
        case .signInFailed: "sign_in_failed"
        case .signedIn: "signed_in"
        case .signedOut: "signed_out"
        case .pairingFailed: "pairing_failed"
        case .computerPaired: "computer_paired"
        case .accessRequested: "access_requested"
        case .accessRequestFailed: "access_request_failed"
        case .notificationsAnswered: "notifications_answered"
        case .notificationOpened: "notification_opened"
        case .settingChanged: "setting_changed"
        case .linkOpened: "link_opened"
        case .voiceCallStarted: "voice_call_started"
        case .voiceCallQuality: "voice_call_quality"
        case .screenViewed: "screen_viewed"
        case .reviewOpportunity: "review_opportunity"
        case .reviewCancelled: "review_cancelled"
        case .reviewRequested: "review_requested"
        case .reviewLinkOpened: "review_link_opened"
        }
    }

    var properties: [String: Analytics.Value] {
        switch self {
        case let .appOpened(notifications, widgets):
            var p: [String: Analytics.Value] = [:]
            p["notifications"] = notifications.map { .string($0.rawValue) }
            p["widgets"] = widgets.map(Analytics.Value.int)
            return p
        case let .onboardingStepViewed(step): return ["step": .string(step.rawValue)]
        case let .onboardingCompleted(via): return ["via": .string(via.rawValue)]
        case let .signInStarted(provider), let .signedIn(provider): return ["provider": .string(provider.rawValue)]
        case let .signInFailed(provider, reason): return ["provider": .string(provider.rawValue), "reason": .string(reason.rawValue)]
        case .signedOut, .accessRequested: return [:]
        case let .pairingFailed(reason): return ["reason": .string(reason.rawValue)]
        case let .computerPaired(via, computer):
            return ["via": .string(via.rawValue), "computer": .string(Analytics.computerTag(computer))]
        case let .accessRequestFailed(reason): return ["reason": .string(reason.rawValue)]
        case let .notificationsAnswered(granted, source): return ["granted": .bool(granted), "source": .string(source.rawValue)]
        case let .notificationOpened(kind): return ["kind": .string(kind.rawValue)]
        case let .settingChanged(setting, on): return ["setting": .string(setting.rawValue), "on": .bool(on)]
        case let .linkOpened(link): return ["link": .string(link.rawValue)]
        case let .voiceCallStarted(engine): return ["engine": .string(engine.rawValue)]
        case let .voiceCallQuality(engine, readyMs, reconnects, stuck, failure):
            var p: [String: Analytics.Value] = ["engine": .string(engine.rawValue), "reconnects": .int(reconnects),
                                                "stuck": .string(stuck.rawValue), "failure": .string(failure.rawValue)]
            p["ready_ms"] = readyMs.map(Analytics.Value.int)
            return p
        case let .reviewOpportunity(completed), let .reviewRequested(completed):
            return ["completed_interactions": .int(completed), "policy": .string("v1"), "trigger": .string("return_to_bots")]
        case let .reviewCancelled(reason): return ["reason": .string(reason.rawValue), "policy": .string("v1")]
        case .reviewLinkOpened: return ["source": .string("settings")]
        case let .screenViewed(screen): return ["screen": .string(screen.rawValue)]
        }
    }
}
