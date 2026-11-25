import Foundation
import Observation

/// Deterministic frame-by-frame text reveal pacer for newly arrived and streaming agent messages.
/// Prevents bursty host flushes from snapping large blocks all at once while guaranteeing bounded
/// catch-up latency and zero replay for historical, cached, or catch-up entries.
struct StreamTextState: Equatable, Sendable {
    static let frameInterval: TimeInterval = 1.0 / 60.0
    static let targetInterval: TimeInterval = 0.028
    static let maxStaticInterval: TimeInterval = 0.090
    static let frameBudget: TimeInterval = 0.012
    static let frameFast: TimeInterval = 0.004
    static let catchUpTarget: TimeInterval = 0.930
    static let maxLag: TimeInterval = 1.070
    static let initialArrivalEstimate: TimeInterval = 0.360
    static let minArrivalSample: TimeInterval = 0.040
    static let maxArrivalSample: TimeInterval = 1.140
    static let windowPad: TimeInterval = 0.115
    static let maxWindow: TimeInterval = 1.360
    static let baseSteps: Int = 57
    static let arrivalHistoryLimit: Int = 4
    static let arrivalRevealMin: TimeInterval = 0.360
    static let arrivalRevealPerChar: TimeInterval = 0.0032
    static let arrivalRevealMax: TimeInterval = 5.200

    struct LagCheckpoint: Equatable, Sendable {
        let count: Int
        let at: TimeInterval
    }

    private(set) var text: String
    private(set) var totalCount: Int
    private(set) var shownCount: Int
    private(set) var displayedText: String
    private(set) var wasLive: Bool
    private(set) var finalAt: TimeInterval?
    private(set) var arrivalRevealAt: TimeInterval?
    private(set) var arrivalRevealUntil: TimeInterval?
    private(set) var tickInterval: TimeInterval
    private(set) var lastTickAt: TimeInterval
    private(set) var lastArrivalAt: TimeInterval?
    private(set) var arrivalEstimate: TimeInterval
    private(set) var recentArrivalIntervals: [TimeInterval]
    private(set) var drainUntil: TimeInterval
    private(set) var checkpoints: [LagCheckpoint]

    var needsTick: Bool { shownCount < totalCount }
    var isStreamingActive: Bool { needsTick || (wasLive && finalAt == nil) }

    static func arrivalRevealDuration(length: Int) -> TimeInterval {
        guard length > 0 else { return 0 }
        let scaled = arrivalRevealMin + Double(length) * arrivalRevealPerChar
        return min(arrivalRevealMax, max(arrivalRevealMin, scaled))
    }

    init(
        text: String,
        live: Bool,
        now: TimeInterval = 0,
        arrivalReveal: Bool = false,
        hidden: Bool = false,
        initialShownCount: Int? = nil
    ) {
        let total = text.count
        self.text = text
        self.totalCount = total
        self.tickInterval = Self.targetInterval
        self.lastTickAt = now
        self.arrivalEstimate = Self.initialArrivalEstimate
        self.recentArrivalIntervals = []

        if !live {
            let clampedInitial = min(max(0, initialShownCount ?? 0), total)
            let remaining = total - clampedInitial
            let duration = (arrivalReveal && !hidden && remaining > 0) ? Self.arrivalRevealDuration(length: remaining) : 0
            if duration > 0 {
                self.shownCount = clampedInitial
                self.displayedText = Self.prefix(text, count: clampedInitial)
                self.wasLive = false
                self.finalAt = nil
                self.arrivalRevealAt = now
                self.arrivalRevealUntil = now + duration
                self.lastArrivalAt = nil
                self.drainUntil = now + duration
                self.checkpoints = []
            } else {
                self.shownCount = total
                self.displayedText = text
                self.wasLive = false
                self.finalAt = nil
                self.arrivalRevealAt = nil
                self.arrivalRevealUntil = nil
                self.lastArrivalAt = nil
                self.drainUntil = now
                self.checkpoints = []
            }
            return
        }

        if hidden {
            self.shownCount = total
            self.displayedText = text
            self.wasLive = true
            self.finalAt = nil
            self.arrivalRevealAt = nil
            self.arrivalRevealUntil = nil
            self.lastArrivalAt = text.isEmpty ? nil : now
            self.drainUntil = now
            self.checkpoints = []
            return
        }

        let clampedInitial = min(max(0, initialShownCount ?? 0), total)
        self.shownCount = clampedInitial
        self.displayedText = Self.prefix(text, count: clampedInitial)
        self.wasLive = true
        self.finalAt = nil
        self.arrivalRevealAt = nil
        self.arrivalRevealUntil = nil
        self.lastArrivalAt = text.isEmpty ? nil : now
        let initialWindow = min(Self.maxWindow, Self.initialArrivalEstimate + Self.windowPad)
        self.drainUntil = text.isEmpty ? now : now + initialWindow
        self.checkpoints = total > clampedInitial ? [LagCheckpoint(count: total, at: now)] : []
    }

    mutating func converge() {
        guard shownCount != totalCount || wasLive || finalAt != nil || arrivalRevealAt != nil || arrivalRevealUntil != nil || !checkpoints.isEmpty else {
            return
        }
        shownCount = totalCount
        displayedText = text
        wasLive = false
        finalAt = nil
        arrivalRevealAt = nil
        arrivalRevealUntil = nil
        lastArrivalAt = nil
        checkpoints = []
    }

    mutating func sync(text nextText: String, live: Bool, now: TimeInterval, hidden: Bool = false) {
        if hidden {
            if text != nextText {
                text = nextText
                totalCount = nextText.count
            }
            converge()
            return
        }

        if nextText == text, live == (wasLive && finalAt == nil) {
            return
        }

        let nextTotal = nextText.count

        if !live, !wasLive {
            if let revealUntil = arrivalRevealUntil, nextTotal >= shownCount, shownCount < nextTotal {
                var aligned = shownCount
                if aligned > 0, nextText != text {
                    aligned = Self.commonPrefixCount(text, nextText, limit: aligned)
                }
                text = nextText
                totalCount = nextTotal
                if aligned >= nextTotal {
                    converge()
                    return
                }
                shownCount = aligned
                displayedText = Self.prefix(nextText, count: aligned)
                if nextTotal > totalCount {
                    let extended = now + Self.arrivalRevealDuration(length: nextTotal - aligned)
                    let updated = max(revealUntil, extended)
                    arrivalRevealUntil = updated
                    drainUntil = updated
                }
                return
            }
            text = nextText
            totalCount = nextTotal
            shownCount = nextTotal
            displayedText = nextText
            wasLive = false
            finalAt = nil
            arrivalRevealAt = nil
            arrivalRevealUntil = nil
            lastArrivalAt = nil
            drainUntil = now
            checkpoints = []
            return
        }

        if nextTotal < shownCount {
            text = nextText
            totalCount = nextTotal
            shownCount = nextTotal
            displayedText = nextText
            wasLive = live
            finalAt = nil
            arrivalRevealAt = nil
            arrivalRevealUntil = nil
            lastArrivalAt = live && nextTotal > 0 ? now : nil
            arrivalEstimate = Self.initialArrivalEstimate
            recentArrivalIntervals = []
            let initialWindow = min(Self.maxWindow, Self.initialArrivalEstimate + Self.windowPad)
            drainUntil = live && nextTotal > 0 ? now + initialWindow : now
            checkpoints = live && nextTotal > 0 ? [LagCheckpoint(count: nextTotal, at: now)] : []
            return
        }

        var aligned = shownCount
        if aligned > 0, nextText != text {
            aligned = Self.commonPrefixCount(text, nextText, limit: aligned)
        }
        if aligned < shownCount {
            checkpoints = checkpoints.filter { $0.count <= aligned }
        }
        let grew = nextTotal > totalCount
        if grew {
            let nextCp = LagCheckpoint(count: nextTotal, at: now)
            if let last = checkpoints.last, now - last.at < 0.016 {
                checkpoints[checkpoints.count - 1] = nextCp
            } else {
                checkpoints.append(nextCp)
            }
        }

        text = nextText
        totalCount = nextTotal
        shownCount = aligned
        displayedText = Self.prefix(nextText, count: aligned)

        if live {
            if grew { updateArrivalTiming(now: now) }
            wasLive = true
            finalAt = nil
            arrivalRevealAt = nil
            arrivalRevealUntil = nil
            return
        }

        if aligned >= nextTotal {
            converge()
            return
        }

        wasLive = true
        finalAt = finalAt ?? now
        arrivalRevealAt = nil
        arrivalRevealUntil = nil
    }

    @discardableResult
    mutating func step(now: TimeInterval, lastFrameCost: TimeInterval = 0, frameDelta: TimeInterval? = nil) -> Bool {
        if shownCount >= totalCount {
            if finalAt != nil || arrivalRevealUntil != nil || (!wasLive && !checkpoints.isEmpty) {
                converge()
            }
            return false
        }

        let remaining = totalCount - shownCount
        tickInterval = Self.adaptTickInterval(current: tickInterval, lastFrameCost: lastFrameCost, backlog: remaining)
        let rawFrame = frameDelta ?? max(Self.frameInterval, tickInterval)
        let effectiveFrame = min(rawFrame, max(tickInterval * 1.5, 0.045))

        var expiredCount = 0
        var activeCheckpoints: [LagCheckpoint] = []
        activeCheckpoints.reserveCapacity(checkpoints.count)
        for cp in checkpoints where cp.count > shownCount {
            if now - cp.at >= Self.maxLag {
                expiredCount = max(expiredCount, cp.count)
            } else {
                activeCheckpoints.append(cp)
            }
        }

        let stepCount: Int
        if let finalAt {
            stepCount = Self.catchUpStep(remaining: remaining, finalAt: finalAt, now: now, frame: effectiveFrame)
        } else if let revealUntil = arrivalRevealUntil {
            let budgetLeft = revealUntil - now
            if budgetLeft <= effectiveFrame {
                stepCount = remaining
            } else {
                let ticksLeft = max(1.0, budgetLeft / max(0.001, effectiveFrame))
                stepCount = max(1, Int((Double(remaining) / ticksLeft).rounded()))
            }
        } else {
            let timeLeft = max(effectiveFrame, drainUntil - now)
            let pacingFrames = max(1.0, timeLeft / max(0.001, effectiveFrame))
            stepCount = max(1, Int((Double(remaining) / pacingFrames).rounded()))
        }

        let expiredCap = (finalAt == nil && arrivalRevealUntil == nil) ? expiredCount : 0
        let nextShown = min(totalCount, max(shownCount + stepCount, expiredCap))
        let changed = nextShown != shownCount

        shownCount = nextShown
        displayedText = Self.prefix(text, count: nextShown)
        lastTickAt = now
        checkpoints = activeCheckpoints.filter { $0.count > nextShown }

        if nextShown >= totalCount {
            if finalAt != nil { wasLive = false }
            finalAt = nil
            arrivalRevealAt = nil
            arrivalRevealUntil = nil
            checkpoints = []
        }
        return changed
    }

    private mutating func updateArrivalTiming(now: TimeInterval) {
        if let prev = lastArrivalAt {
            let dt = now - prev
            if dt >= Self.minArrivalSample {
                let sample = min(Self.maxArrivalSample, max(Self.minArrivalSample, dt))
                recentArrivalIntervals.append(sample)
                if recentArrivalIntervals.count > Self.arrivalHistoryLimit {
                    recentArrivalIntervals.removeFirst(recentArrivalIntervals.count - Self.arrivalHistoryLimit)
                }
                let target = recentArrivalIntervals.count >= Self.arrivalHistoryLimit
                    ? (recentArrivalIntervals.max() ?? Self.initialArrivalEstimate)
                    : max(Self.initialArrivalEstimate, recentArrivalIntervals.max() ?? Self.initialArrivalEstimate)
                arrivalEstimate = target >= arrivalEstimate ? target : arrivalEstimate + (target - arrivalEstimate) * 0.4
                lastArrivalAt = now
            }
        } else {
            lastArrivalAt = now
        }
        let window = min(Self.maxWindow, arrivalEstimate + Self.windowPad)
        drainUntil = max(drainUntil, now + window)
    }

    private static func catchUpStep(remaining: Int, finalAt: TimeInterval, now: TimeInterval, frame: TimeInterval) -> Int {
        guard remaining > 0 else { return 0 }
        let base = max(1, Int(ceil(Double(remaining) / Double(max(1, baseSteps)))))
        let elapsed = max(0, now - finalAt)
        let effectiveFrame = max(0.001, frame)
        let budgetLeft = catchUpTarget - elapsed
        if budgetLeft <= effectiveFrame { return remaining }
        let framesLeft = max(1.0, budgetLeft / effectiveFrame)
        return max(base, Int(ceil(Double(remaining) / framesLeft)))
    }

    private static func adaptTickInterval(current: TimeInterval, lastFrameCost: TimeInterval, backlog: Int) -> TimeInterval {
        if lastFrameCost > frameBudget {
            return min(maxStaticInterval, current * 1.5)
        }
        if lastFrameCost < frameFast, backlog > 0 {
            return max(targetInterval, current - 0.004)
        }
        return current
    }

    private static func prefix(_ text: String, count: Int) -> String {
        if count <= 0 { return "" }
        if count >= text.count { return text }
        let end = text.index(text.startIndex, offsetBy: count)
        return String(text[..<end])
    }

    private static func commonPrefixCount(_ a: String, _ b: String, limit: Int) -> Int {
        var count = 0
        for (ca, cb) in zip(a, b) {
            if count >= limit || ca != cb { break }
            count += 1
        }
        return count
    }
}

/// Main-actor controller driving `StreamTextState` ticks for live SwiftUI message views.
@MainActor
@Observable
final class StreamTextController {
    private(set) var state: StreamTextState
    private var tickTask: Task<Void, Never>?
    private var onProgress: ((Int) -> Void)?
    private var onComplete: ((Int) -> Void)?

    var displayedText: String { state.displayedText }
    var isStreamingActive: Bool { state.isStreamingActive }
    var needsTick: Bool { state.needsTick }

    init(
        text: String,
        live: Bool,
        arrivalReveal: Bool,
        hidden: Bool = false,
        initialShownCount: Int? = nil,
        onProgress: ((Int) -> Void)? = nil,
        onComplete: ((Int) -> Void)? = nil
    ) {
        self.onProgress = onProgress
        self.onComplete = onComplete
        self.state = StreamTextState(
            text: text,
            live: live,
            now: Self.now(),
            arrivalReveal: arrivalReveal,
            hidden: hidden,
            initialShownCount: initialShownCount
        )
        scheduleTickIfNeeded()
    }

    func sync(
        text: String,
        live: Bool,
        hidden: Bool = false,
        onProgress: ((Int) -> Void)? = nil,
        onComplete: ((Int) -> Void)? = nil
    ) {
        if let onProgress { self.onProgress = onProgress }
        if let onComplete { self.onComplete = onComplete }
        state.sync(text: text, live: live, now: Self.now(), hidden: hidden)
        if !state.needsTick {
            stop()
            self.onComplete?(state.totalCount)
        } else {
            scheduleTickIfNeeded()
        }
    }

    func converge() {
        stop()
        state.converge()
        onComplete?(state.totalCount)
    }

    func stop() {
        tickTask?.cancel()
        tickTask = nil
    }

    private func scheduleTickIfNeeded() {
        guard state.needsTick else {
            stop()
            return
        }
        guard tickTask == nil else { return }
        tickTask = Task { [weak self] in
            while let self, !Task.isCancelled, self.state.needsTick {
                let intervalMs = Int((self.state.tickInterval * 1000).rounded())
                try? await Task.sleep(for: .milliseconds(max(16, intervalMs)))
                guard !Task.isCancelled else { break }
                let start = Self.now()
                let changed = self.state.step(now: start)
                if changed {
                    self.onProgress?(self.state.shownCount)
                }
                if !self.state.needsTick {
                    self.onComplete?(self.state.totalCount)
                    break
                }
            }
            self?.tickTask = nil
        }
    }

    private static func now() -> TimeInterval {
        ProcessInfo.processInfo.systemUptime
    }
}
