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
    /// The signed-out computers moved into the account at sign-in: with this phone's keys, or to be scanned again.
    case signedOutPairingsMoved(computers: Int, keys: MovedKeys)
    case continuedWithoutAccount
    case computerFoundOnNetwork
    case addToAccountAsked
    case addToAccountCompleted
    case addToAccountFailed(AddToAccountFailure)
    /// The user gave Codync Cloud the key to their own account with a cloud provider (`provider`: the cloud's id for it,
    /// such as `boat`); never the key or its hint.
    case cloudKeySaved(CloudKeyOutcome, provider: String)
    /// The user made another Codync Cloud computer on `provider` (the cloud's id for it); never its name.
    case cloudComputerCreated(CloudCreateOutcome, provider: String)
    /// The user opened `provider`'s free plan from its key sheet (the cloud's id for it).
    case cloudOfferOpened(provider: String)
    /// The user renamed one of their Codync Cloud computers; never the name.
    case cloudComputerRenamed(CloudRenameOutcome)
    /// The user deleted one of their Codync Cloud computers (`provider`: the cloud's id for the one it ran on).
    case cloudComputerDeleted(CloudDeleteOutcome, provider: String)
    /// The user deleted a Codync Cloud computer's snapshots (`provider`: the cloud's id for the one it runs on).
    case cloudSnapshotsDeleted(CloudDeleteOutcome, provider: String)
    /// The user removed the key to their own account with `provider` (the cloud's id for it).
    case cloudKeyRemoved(CloudRemoveOutcome, provider: String)
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
    case reviewRequested(prompts: Int)
    case reviewLinkOpened
    /// The What's new window is on screen after an update (docs/features/whats-new.md).
    case whatsNewShown
    /// A dev server opened, or its address copied; the host reports what it does itself.
    case devServerAction(DevServerUse)

    public enum Notifications: String, Sendable { case allowed, denied, notAsked = "not_asked" }
    public enum OnboardingStep: String, Sendable { case welcome, choose, install, scan }
    public enum OnboardingVia: String, Sendable { case paired, account, skipped }
    public enum SignInProvider: String, Sendable { case apple, google, github }
    public enum SignInFailure: String, Sendable { case cancelled, verification, error }
    public enum MovedKeys: String, Sendable { case moved, rescan }
    public enum PairVia: String, Sendable { case qr, link, account }
    public enum AccessFailure: String, Sendable { case denied, expired, cancelled }
    public enum PromptSource: String, Sendable { case settings, widgets }
    public enum Setting: String, Sendable { case push, liveActivities = "live_activities" }
    public enum Link: String, Sendable {
        case github, issues, appStore = "app_store", demo, tailscale, privacy, installCommand = "install_command", installLink = "install_link"
    }
    public enum DevServerUse: String, Sendable { case open, copy }
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

    /// `saved`; `refused`: the cloud or the provider turned the key down (what to fix was shown); `failed`: anything else.
    public enum CloudKeyOutcome: String, Sendable {
        case saved, refused, failed

        public init(_ error: any Error) {
            self = (error as? CloudError)?.code == "keyRefused" ? .refused : .failed
        }
    }

    /// `created`; `refused`: the cloud said what to fix (`409`: at the limit or no key, or the provider turned the key
    /// down); `failed`: anything else.
    public enum CloudCreateOutcome: String, Sendable {
        case created, refused, failed

        public init(_ error: any Error) {
            let cloud = error as? CloudError
            self = cloud?.status == 409 || cloud?.code == "keyRefused" ? .refused : .failed
        }
    }

    public enum CloudRenameOutcome: String, Sendable {
        case renamed, failed
    }

    /// `deleted`; `refused`: the cloud said no (`409`: its snapshots while it sleeps); `failed`: anything else.
    public enum CloudDeleteOutcome: String, Sendable {
        case deleted, refused, failed

        public init(_ error: any Error) {
            self = (error as? CloudError)?.status == 409 ? .refused : .failed
        }
    }

    /// `removed`; `refused`: the cloud said no (`409`: a computer still runs on that provider); `failed`: anything else.
    public enum CloudRemoveOutcome: String, Sendable {
        case removed, refused, failed

        public init(_ error: any Error) {
            self = (error as? CloudError)?.status == 409 ? .refused : .failed
        }
    }

    public enum AddToAccountFailure: String, Sendable {
        case declined, certificate, unreachable, other

        public init(_ error: any Error) {
            switch error {
            case let host as HostError where host.isTransient: self = .unreachable
            case let HostError.http(_, message) where message == "The computer stayed out of this account.": self = .declined
            case let HostError.http(_, message) where message.hasPrefix("Couldn't confirm your Codync account"): self = .certificate
            case let cloud as CloudError where cloud.code == "noCertificate": self = .certificate
            case let cloud as CloudError where cloud.isTransient: self = .unreachable
            case is URLError: self = .unreachable
            default: self = .other
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
                case .unreachableDirectly: .unreachable
                // The host turns down a pairing code that expired or was already used, and a phone it doesn't
                // let in: another account's, one it can't confirm, or one its user declined.
                case let .http(403, message): message.hasPrefix("This pairing code expired") ? .expired : .unauthorized
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
        case .signedOutPairingsMoved: "signed_out_pairings_moved"
        case .continuedWithoutAccount: "continued_without_account"
        case .computerFoundOnNetwork: "computer_found_on_network"
        case .addToAccountAsked: "add_to_account_asked"
        case .addToAccountCompleted: "add_to_account_completed"
        case .addToAccountFailed: "add_to_account_failed"
        case .cloudKeySaved: "cloud_key_saved"
        case .cloudComputerCreated: "cloud_computer_created"
        case .cloudOfferOpened: "cloud_offer_opened"
        case .cloudComputerRenamed: "cloud_computer_renamed"
        case .cloudComputerDeleted: "cloud_computer_deleted"
        case .cloudSnapshotsDeleted: "cloud_snapshots_deleted"
        case .cloudKeyRemoved: "cloud_key_removed"
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
        case .reviewRequested: "review_requested"
        case .reviewLinkOpened: "review_link_opened"
        case .whatsNewShown: "whats_new_shown"
        case .devServerAction: "dev_server_action"
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
        case .signedOut, .accessRequested, .continuedWithoutAccount, .computerFoundOnNetwork, .addToAccountAsked,
             .addToAccountCompleted, .whatsNewShown: return [:]
        case let .addToAccountFailed(reason): return ["reason": .string(reason.rawValue)]
        case let .cloudKeySaved(outcome, provider): return ["outcome": .string(outcome.rawValue), "provider": .string(provider)]
        case let .cloudComputerCreated(outcome, provider): return ["outcome": .string(outcome.rawValue), "provider": .string(provider)]
        case let .cloudOfferOpened(provider): return ["provider": .string(provider)]
        case let .cloudComputerRenamed(outcome): return ["outcome": .string(outcome.rawValue)]
        case let .cloudComputerDeleted(outcome, provider), let .cloudSnapshotsDeleted(outcome, provider):
            return ["outcome": .string(outcome.rawValue), "provider": .string(provider)]
        case let .cloudKeyRemoved(outcome, provider): return ["outcome": .string(outcome.rawValue), "provider": .string(provider)]
        case let .signedOutPairingsMoved(computers, keys): return ["computers": .int(computers), "keys": .string(keys.rawValue)]
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
        case let .reviewRequested(prompts):
            return ["prompts": .int(prompts), "policy": .string("v2")]
        case .reviewLinkOpened: return ["source": .string("settings")]
        case let .devServerAction(use): return ["action": .string(use.rawValue)]
        case let .screenViewed(screen): return ["screen": .string(screen.rawValue)]
        }
    }
}
