import Foundation
import Observation
import os

/// The iPhone's StoreKit invitation state; never sent to the host or shared with widgets.
@MainActor @Observable
public final class ReviewInvitation {
    public static let shared = ReviewInvitation()
    static let storageKey = "reviewInvitation.v1"
    private static let log = Logger(subsystem: "com.pokai.Codync", category: "ReviewInvitation")
    private let defaults: UserDefaults
    private let capture: @MainActor (AnalyticsEvent) -> Void
    private var history: ReviewHistory
    private var activeSince: Date?
    private var viewedChat: String?
    public private(set) var opportunity: UUID?
    private var interruptions: Set<UUID> = []
    public var isInterrupted: Bool { !interruptions.isEmpty }

    init(defaults: UserDefaults = .standard, now: Date = .now,
         capture: @escaping @MainActor (AnalyticsEvent) -> Void = { Analytics.shared.capture($0) }) {
        self.defaults = defaults
        self.capture = capture
        if let data = defaults.data(forKey: Self.storageKey) {
            do { history = try JSONDecoder().decode(ReviewHistory.self, from: data) }
            catch {
                Self.log.error("Cannot read review history: \(error.localizedDescription)")
                history = ReviewHistory(firstUse: now)
            }
        } else { history = ReviewHistory(firstUse: now) }
        prune(at: now)
        save()
    }

    public func setInterrupted(_ token: UUID, active: Bool) {
        if active {
            interruptions.insert(token)
            cancel(.unavailable)
        } else { interruptions.remove(token) }
    }

    public func setActive(_ active: Bool, now: Date = .now) {
        if active {
            prune(at: now)
            save()
            if activeSince == nil { activeSince = now }
        } else {
            activeSince = nil
            viewedChat = nil
            cancel(.background)
        }
    }

    public func sent(_ nonce: String, now: Date = .now) {
        prune(at: now)
        if history.sent.count >= 100, let oldest = history.sent.min(by: { $0.value < $1.value })?.key {
            history.sent[oldest] = nil
        }
        history.sent[nonce] = now
        history.recordActivity(at: now)
        viewedChat = nil
        cancel(.interaction)
        save()
    }

    /// Called only after an unobscured final answer at the bottom of the chat stayed visible.
    public func viewed(chat: String, entries: [Entry], now: Date = .now) {
        guard activeSince != nil, !isInterrupted,
              let result = ReviewCompletion.match(entries: entries, sent: history.sent, now: now) else { return }
        for nonce in result { history.sent[nonce] = nil }
        history.completed += 1
        history.recordActivity(at: now)
        viewedChat = chat
        save()
    }

    public func returned(from chat: String, version: String = AppVersion.current, now: Date = .now) {
        defer { viewedChat = nil }
        guard !isInterrupted, viewedChat == chat, let activeSince, now.timeIntervalSince(activeSince) >= 60,
              history.eligible(at: now, version: version) else { return }
        opportunity = UUID()
        capture(.reviewOpportunity(completed: history.completed))
    }

    public func cancel(_ reason: AnalyticsEvent.ReviewCancellation) {
        guard opportunity != nil else { return }
        opportunity = nil
        capture(.reviewCancelled(reason))
    }

    /// Persist before StoreKit: an undisplayed request still consumes our own cooldown.
    public func request(_ token: UUID, version: String = AppVersion.current, now: Date = .now) -> Bool {
        guard !isInterrupted, opportunity == token, let activeSince, now.timeIntervalSince(activeSince) >= 60,
              history.eligible(at: now, version: version) else { return false }
        opportunity = nil
        history.recordRequest(at: now, version: version)
        save()
        capture(.reviewRequested(completed: history.completed))
        return true
    }

    private func prune(at now: Date) {
        history.sent = history.sent.filter { now.timeIntervalSince($0.value) < 30 * ReviewHistory.day }
    }

    private func save() {
        do { defaults.set(try JSONEncoder().encode(history), forKey: Self.storageKey) }
        catch { Self.log.error("Cannot save review history: \(error.localizedDescription)") }
    }
}
