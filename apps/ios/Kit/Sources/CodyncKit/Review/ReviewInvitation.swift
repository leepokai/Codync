import Foundation
import Observation
import os

/// The iPhone's StoreKit invitation state; never sent to the host or shared with widgets.
@MainActor @Observable
public final class ReviewInvitation {
    public static let shared = ReviewInvitation()
    static let storageKey = "reviewInvitation.v1"
    /// E2E flows launch with this argument on: the system's window would cover what they tap.
    static let offKey = "reviewInvitationOff"
    private static let log = Logger(subsystem: "com.pokai.Codync", category: "ReviewInvitation")
    private let defaults: UserDefaults
    private let capture: @MainActor (AnalyticsEvent) -> Void
    private var history: ReviewHistory
    public private(set) var opportunity: UUID?
    private var opportunityDate = Date.distantPast
    /// With no view there to ask (a background shortcut), the token waits in memory: coming back to the app later must not ask.
    private static let opportunityLifetime: TimeInterval = 5

    init(defaults: UserDefaults = .standard,
         capture: @escaping @MainActor (AnalyticsEvent) -> Void = { Analytics.shared.capture($0) }) {
        self.defaults = defaults
        self.capture = capture
        if let data = defaults.data(forKey: Self.storageKey) {
            do { history = try JSONDecoder().decode(ReviewHistory.self, from: data) }
            catch {
                Self.log.error("Cannot read review history: \(error.localizedDescription)")
                history = ReviewHistory()
            }
        } else { history = ReviewHistory() }
    }

    /// A prompt reached the computer or its mailbox.
    public func sent(version: String = AppVersion.current, now: Date = .now) {
        history.prompts += 1
        save()
        if !defaults.bool(forKey: Self.offKey), history.eligible(at: now, version: version) {
            opportunity = UUID()
            opportunityDate = now
        }
    }

    /// Persist before StoreKit: an undisplayed request still consumes our own cooldown.
    public func request(_ token: UUID, version: String = AppVersion.current, now: Date = .now) -> Bool {
        guard opportunity == token else { return false }
        opportunity = nil
        guard now.timeIntervalSince(opportunityDate) <= Self.opportunityLifetime else { return false }
        history.recordRequest(at: now, version: version)
        save()
        capture(.reviewRequested(prompts: history.prompts))
        return true
    }

    /// The app couldn't ask on this one (backgrounded, or its view went away): only a later prompt asks.
    public func skip(_ token: UUID) {
        if opportunity == token { opportunity = nil }
    }

    private func save() {
        do { defaults.set(try JSONEncoder().encode(history), forKey: Self.storageKey) }
        catch { Self.log.error("Cannot save review history: \(error.localizedDescription)") }
    }
}
