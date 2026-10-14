import CryptoKit
import Foundation

/// What the iPhone reports. Bot and message events come from the computer's host instead.
public enum AnalyticsEvent: String, Sendable {
    case appOpened = "app_opened"
    case onboardingCompleted = "onboarding_completed"
    case computerPaired = "computer_paired"
    case signedIn = "signed_in"
    case signedOut = "signed_out"
    case voiceCallStarted = "voice_call_started"
}

/// Product analytics: batches events to PostHog, on unless turned off in Settings.
/// Only the app calls it; widgets and the notification extension never do.
@MainActor
public final class Analytics {
    public static let shared = Analytics()

    /// One event in PostHog's `/batch/` body.
    struct Event: Encodable, Equatable {
        let uuid = UUID()
        let event: String
        let distinct_id: String
        let timestamp: String
        let properties: [String: Value]
    }

    enum Value: Encodable, Equatable, Sendable {
        case string(String), bool(Bool), int(Int), array([Value]), object([String: Value])

        func encode(to encoder: Encoder) throws {
            var c = encoder.singleValueContainer()
            switch self {
            case .string(let s): try c.encode(s)
            case .bool(let b): try c.encode(b)
            case .int(let i): try c.encode(i)
            case .array(let a): try c.encode(a)
            case .object(let o): try c.encode(o)
            }
        }
    }

    private struct Batch: Encodable {
        let api_key: String
        let batch: [Event]
    }

    nonisolated static let endpoint = URL(string: "https://us.i.posthog.com/batch/")!
    static let apiKey = "phc_rQHvxdSFKnMquPfMjmiMhXhM4tqJ5qCYsWuVEoFxCCWX"
    /// `Bool`, absent = on.
    public static let enabledKey = "analyticsEnabled"
    private static let installKey = "analyticsInstallID"
    private static let identifiedKey = "analyticsIdentifiedUsers"
    private static let batchSize = 20
    private static let maxPending = 500
    private static let flushDelay = Duration.seconds(30)

    /// The signed-in Codync account, read at capture time; nil = anonymous install ID.
    public var userID: @MainActor () -> String? = { nil }
    private let defaults: UserDefaults
    private let upload: @Sendable (Data) async -> Bool
    private(set) var pending: [Event] = []
    private var timer: Task<Void, Never>?
    private var sending = false
    private var foreground = false

    init(defaults: UserDefaults = .standard, upload: @escaping @Sendable (Data) async -> Bool = Analytics.post) {
        self.defaults = defaults
        self.upload = upload
    }

    public var isEnabled: Bool { defaults.object(forKey: Self.enabledKey) as? Bool ?? true }

    /// Turning it off drops everything not yet sent.
    public func setEnabled(_ enabled: Bool) {
        defaults.set(enabled, forKey: Self.enabledKey)
        guard !enabled else { return flush() }
        pending = []
        timer?.cancel()
        timer = nil
    }

    /// `properties` are public names, never content.
    public func capture(_ event: AnalyticsEvent, _ properties: [String: String] = [:]) {
        record(event.rawValue, properties.mapValues(Value.string))
    }

    /// One event for the signed-in account (merging the install ID into it once), else the install ID.
    func record(_ name: String, _ extra: [String: Value]) {
        guard isEnabled else { return }
        if let user = userID() {
            var identified = defaults.stringArray(forKey: Self.identifiedKey) ?? []
            if !identified.contains(user) {
                enqueue("$identify", user, ["$anon_distinct_id": .string(installID)])
                identified.append(user)
                defaults.set(identified, forKey: Self.identifiedKey)
            }
            enqueue(name, user, extra)
        } else {
            enqueue(name, installID, extra)
        }
    }

    /// A computer's analytics key, the same hash its host sends: joins `computer_paired` with that
    /// computer's events without revealing the id the relay knows it by.
    public static func computerTag(_ id: ComputerID) -> String {
        SHA256.hash(data: Data("codync-analytics:\(id)".utf8)).prefix(8).map { String(format: "%02x", $0) }.joined()
    }

    /// `app_opened` once per trip to the foreground (`.inactive` blips don't count).
    public func becameActive() {
        guard !foreground else { return }
        foreground = true
        capture(.appOpened)
    }

    public func enteredBackground() {
        foreground = false
        flush()
    }

    func flush() {
        timer?.cancel()
        timer = nil
        guard !sending, !pending.isEmpty, isEnabled,
              let body = try? JSONEncoder().encode(Batch(api_key: Self.apiKey, batch: pending)) else { return }
        let sent = Set(pending.map(\.uuid))
        sending = true
        Task {
            let ok = await upload(body)
            sending = false
            if ok { pending.removeAll { sent.contains($0.uuid) } }
            if !pending.isEmpty { scheduleFlush() }
        }
    }

    private var installID: String {
        if let id = defaults.string(forKey: Self.installKey) { return id }
        let id = UUID().uuidString
        defaults.set(id, forKey: Self.installKey)
        return id
    }

    private func enqueue(_ name: String, _ distinctID: String, _ extra: [String: Value]) {
        var properties = Self.common.merging(extra) { $1 }
        properties["$process_person_profile"] = .bool(true)
        pending.append(Event(event: name, distinct_id: distinctID,
                             timestamp: Date.now.ISO8601Format(), properties: properties))
        if pending.count > Self.maxPending { pending.removeFirst(pending.count - Self.maxPending) }
        if pending.count >= Self.batchSize { flush() } else { scheduleFlush() }
    }

    private func scheduleFlush() {
        guard timer == nil else { return }
        timer = Task { [weak self] in
            try? await Task.sleep(for: Self.flushDelay)
            guard !Task.isCancelled else { return }
            self?.timer = nil
            self?.flush()
        }
    }

    static let common: [String: Value] = {
        let os = ProcessInfo.processInfo.operatingSystemVersion
        // The Codync Dev app is a release build too: its environment, not DEBUG, says which.
        #if DEBUG
        let env = "dev"
        #else
        let env = AppEnvironment.current == .main ? "main" : "dev"
        #endif
        return [
            "$lib": .string("codync-ios"),
            "app_version": .string(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? ""),
            "env": .string(env),
            "$os": .string("iOS"),
            "$os_version": .string("\(os.majorVersion).\(os.minorVersion).\(os.patchVersion)"),
        ]
    }()

    /// Sent (or rejected for good) → true; network errors, 429 and 5xx → false, retried next flush.
    nonisolated static func post(_ body: Data) async -> Bool {
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = body
        guard let (_, response) = try? await URLSession.shared.data(for: request),
              let status = (response as? HTTPURLResponse)?.statusCode else { return false }
        return status < 500 && status != 429
    }
}
