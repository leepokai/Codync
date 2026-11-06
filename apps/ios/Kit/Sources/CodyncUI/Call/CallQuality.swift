import CodyncKit
import Foundation

/// How each engine of a call fares, for analytics (never content): one `voice_call_quality` per engine, sent when it's
/// replaced or the call ends — milliseconds until it was ready, reconnects, the first phase stuck too long, why it
/// failed.
@MainActor
final class CallQuality {
    /// Connecting (or reconnecting) this long counts as stuck.
    static let stuckAfter: Duration = .seconds(20)
    private let now: () -> ContinuousClock.Instant
    private let capture: @MainActor (AnalyticsEvent) -> Void
    private var engine: AnalyticsEvent.VoiceEngine?
    private var startedAt: ContinuousClock.Instant
    private var readyMs: Int?
    private var reconnects = 0
    private var stuck = AnalyticsEvent.VoiceStuck.none
    private var failure = AnalyticsEvent.VoiceFailure.none
    private var last: VoicePhase = .starting
    /// When the current connecting phase began.
    private var since: ContinuousClock.Instant

    init(capture: @escaping @MainActor (AnalyticsEvent) -> Void = { Analytics.shared.capture($0) },
         now: @escaping () -> ContinuousClock.Instant = { .now }) {
        self.capture = capture
        self.now = now
        startedAt = now()
        since = startedAt
    }

    /// A new engine takes the call; the previous one's report goes out.
    func begin(_ engine: AnalyticsEvent.VoiceEngine) {
        report()
        self.engine = engine
        startedAt = now()
        since = startedAt
        readyMs = nil
        reconnects = 0
        stuck = .none
        failure = .none
        last = .starting
    }

    func phase(_ phase: VoicePhase) {
        guard engine != nil, phase != last else { return }
        if last.isConnecting { checkStuck() }
        if phase.isActive {
            if readyMs == nil { readyMs = Int((now() - startedAt) / .milliseconds(1)) }
            if last == .reconnecting { reconnects += 1 }
        } else if phase.isConnecting {
            since = now()
        } else if case .failed(_, let kind) = phase {
            failure = AnalyticsEvent.VoiceFailure(rawValue: kind.rawValue) ?? .connection
        }
        last = phase
    }

    /// Playback stopped reporting progress after the voice's turn (the engine's watchdog took the call back).
    func stalled() {
        if stuck == .none { stuck = .speaking }
    }

    /// The engine is done (replaced, or the call ended): its report.
    func report() {
        guard let engine else { return }
        if last.isConnecting { checkStuck() }
        capture(.voiceCallQuality(engine, readyMs: readyMs, reconnects: reconnects, stuck: stuck, failure: failure))
        self.engine = nil
    }

    private func checkStuck() {
        guard stuck == .none, now() - since >= Self.stuckAfter else { return }
        stuck = last == .starting ? .connecting : .reconnecting
    }
}
