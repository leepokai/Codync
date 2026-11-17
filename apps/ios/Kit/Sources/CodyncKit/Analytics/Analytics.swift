import CryptoKit
import Foundation

/// Product analytics: batches events to PostHog, on unless turned off in Settings.
/// Only the app calls it; widgets and the notification extension never do, and the share extension leaves its
/// events for the app (`queueShare`).
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
    private static let sessionKey = "analyticsSession"
    private static let batchSize = 20
    private static let maxPending = 500
    private static let flushDelay = Duration.seconds(30)

    /// The signed-in Codync account, read at capture time; nil = anonymous install ID.
    public var userID: @MainActor () -> String? = { nil }
    /// What `app_opened` says about this iPhone, read on each trip to the foreground; nil = unknown.
    public var foregroundState: @MainActor () async -> (notifications: AnalyticsEvent.Notifications?, widgets: Int?) = { (nil, nil) }
    private let defaults: UserDefaults
    private let upload: @Sendable (Data) async -> Bool
    private(set) var pending: [Event] = []
    private var timer: Task<Void, Never>?
    /// The batch on its way.
    private var sending: Task<Void, Never>?
    private var foreground = false
    private var session = AnalyticsSession()
    /// The session the last launch ended in: where a crash MetricKit reports now happened.
    private let lastLaunchSession: String?

    init(defaults: UserDefaults = .standard, upload: @escaping @Sendable (Data) async -> Bool = Analytics.post) {
        self.defaults = defaults
        self.upload = upload
        lastLaunchSession = defaults.string(forKey: Self.sessionKey)
    }

    public var isEnabled: Bool { defaults.object(forKey: Self.enabledKey) as? Bool ?? true }

    /// Turning it off drops everything not yet sent.
    public func setEnabled(_ enabled: Bool) {
        defaults.set(enabled, forKey: Self.enabledKey)
        guard !enabled else {
            flush()
            return
        }
        pending = []
        timer?.cancel()
        timer = nil
    }

    public func capture(_ event: AnalyticsEvent) {
        record(event.name, event.properties)
    }

    func record(_ name: String, _ extra: [String: Value]) {
        guard isEnabled else { return }
        record(name, extra, sessionID: currentSession())
    }

    /// A crash MetricKit reported from the last launch, in the session that launch ended in.
    func recordCrash(_ properties: [String: Value]) {
        guard isEnabled else { return }
        record("$exception", properties, sessionID: lastLaunchSession)
    }

    /// One event for the signed-in account, else the install ID. The install ID merges into the first
    /// account only: PostHog refuses a second, so a later account on this phone starts its own profile.
    private func record(_ name: String, _ extra: [String: Value], sessionID: String?) {
        if let user = userID() {
            if defaults.object(forKey: Self.identifiedKey) == nil {
                enqueue("$identify", user, ["$anon_distinct_id": .string(installID)], sessionID)
                defaults.set([user], forKey: Self.identifiedKey)
            }
            enqueue(name, user, extra, sessionID)
        } else {
            enqueue(name, installID, extra, sessionID)
        }
    }

    /// A computer's analytics key, the same hash its host sends: joins `computer_paired` with that
    /// computer's events without revealing the id the relay knows it by.
    nonisolated public static func computerTag(_ id: ComputerID) -> String {
        SHA256.hash(data: Data("codync-analytics:\(id)".utf8)).prefix(8).map { String(format: "%02x", $0) }.joined()
    }

    /// Shares the share extension finished, waiting for the app (`queueShare`).
    nonisolated static let sharesKey = "analyticsPendingShares"

    /// From the share extension, which has neither this install's ID nor the Settings switch: kept in the App Group
    /// (the latest 50) until the app's next trip to the foreground sends it as its own.
    nonisolated public static func queueShare(_ share: AnalyticsEvent.ShareSent,
                                              in shared: UserDefaults? = UserDefaults(suiteName: SharedStore.appGroup)) {
        guard let shared else { return }
        let queued = shared.data(forKey: sharesKey).flatMap { try? JSONDecoder().decode([AnalyticsEvent.ShareSent].self, from: $0) } ?? []
        shared.set(try? JSONEncoder().encode(Array((queued + [share]).suffix(50))), forKey: sharesKey)
    }

    /// The extension's shares, captured now (or dropped with analytics off).
    func takeQueuedShares(from shared: UserDefaults?) {
        guard let shared, let data = shared.data(forKey: Self.sharesKey) else { return }
        shared.removeObject(forKey: Self.sharesKey)
        for share in (try? JSONDecoder().decode([AnalyticsEvent.ShareSent].self, from: data)) ?? [] { capture(.shareSent(share)) }
    }

    /// `app_opened` once per trip to the foreground (`.inactive` blips don't count).
    public func becameActive() {
        guard !foreground else { return }
        foreground = true
        takeQueuedShares(from: UserDefaults(suiteName: SharedStore.appGroup))
        Task {
            let state = await foregroundState()
            capture(.appOpened(notifications: state.notifications, widgets: state.widgets))
        }
    }

    /// Sends everything queued and returns once it's sent or failed; the app holds background time for it.
    public func enteredBackground() async {
        foreground = false
        await sending?.value
        await flush()?.value
    }

    /// The upload it started; nil when nothing needs to (or can) go now.
    @discardableResult
    func flush() -> Task<Void, Never>? {
        timer?.cancel()
        timer = nil
        guard sending == nil, !pending.isEmpty, isEnabled,
              let body = try? JSONEncoder().encode(Batch(api_key: Self.apiKey, batch: pending)) else { return nil }
        let sent = Set(pending.map(\.uuid))
        let task = Task {
            let ok = await upload(body)
            sending = nil
            if ok { pending.removeAll { sent.contains($0.uuid) } }
            if !pending.isEmpty { scheduleFlush() }
        }
        sending = task
        return task
    }

    private var installID: String {
        if let id = defaults.string(forKey: Self.installKey) { return id }
        let id = UUID().uuidString
        defaults.set(id, forKey: Self.installKey)
        return id
    }

    /// Kept for the next launch, whose crash reports belong to it.
    private func currentSession() -> String {
        let id = session.id(at: .now)
        defaults.set(id, forKey: Self.sessionKey)
        return id
    }

    private func enqueue(_ name: String, _ distinctID: String, _ extra: [String: Value], _ sessionID: String?) {
        var properties = Self.common.merging(extra) { $1 }
        properties["$process_person_profile"] = .bool(true)
        properties["$session_id"] = sessionID.map(Value.string)
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
