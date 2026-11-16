import CodyncKit
import QuartzCore
import SwiftUI

extension View {
    /// Reports how smoothly each of the reader's scrolls of this scroll view drew; `rows`: how many it holds.
    public func scrollHitches(_ list: PhonePerformance.Scroll.List, rows: Int,
                              report: @escaping @MainActor (PhonePerformance.Scroll) -> Void) -> some View {
        modifier(ScrollHitchTracking(list: list, rows: rows, report: report))
    }
}

private struct ScrollHitchTracking: ViewModifier {
    @State private var meter: ScrollHitchMeter
    let rows: Int

    init(list: PhonePerformance.Scroll.List, rows: Int, report: @escaping @MainActor (PhonePerformance.Scroll) -> Void) {
        _meter = State(initialValue: ScrollHitchMeter(list, report: report))
        self.rows = rows
    }

    func body(content: Content) -> some View {
        content
            .onScrollPhaseChange { _, phase in
                switch phase {
                // `.tracking` is a touch that may never move: only a drag opens a window.
                case .interacting: meter.begin()
                case .idle: meter.rest(rows: rows)
                case .tracking, .decelerating, .animating: break
                }
            }
            .onDisappear { meter.cancel() }
    }
}

/// Times the frames of one of the reader's scrolls: from the drag until `tail` after the list comes
/// to rest, so a jump or second bounce at the stop counts too. A chat's `track` gives where the row on screen at rest
/// now sits: how far it moves during the tail is the scroll's `jump`.
@MainActor
final class ScrollHitchMeter {
    static let tail: Duration = .milliseconds(300)

    private let list: PhonePerformance.Scroll.List
    private let report: @MainActor (PhonePerformance.Scroll) -> Void
    private var link: CADisplayLink?
    private var frames = ScrollFrames()
    private var started: CFTimeInterval = 0
    private var last: CFTimeInterval?
    private var resting: Task<Void, Never>?
    private var track: (() -> CGFloat?)?
    private var restingAt: CGFloat?
    private var jump: CGFloat = 0

    init(_ list: PhonePerformance.Scroll.List, report: @escaping @MainActor (PhonePerformance.Scroll) -> Void) {
        self.list = list
        self.report = report
    }

    /// The reader started dragging: a new window, or the same one when it came during the last one's tail.
    func begin() {
        resting?.cancel()
        resting = nil
        guard link == nil else { return }
        frames = ScrollFrames()
        started = CACurrentMediaTime()
        last = nil
        let link = CADisplayLink(target: self, selector: #selector(frame))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    /// The list came to rest: the window closes and is reported after the tail.
    func rest(rows: Int, stop: PhonePerformance.Scroll.Stop? = nil, track: (() -> CGFloat?)? = nil) {
        guard link != nil, resting == nil else { return }
        self.track = track
        restingAt = track?()
        jump = 0
        resting = Task { [weak self] in
            try? await Task.sleep(for: Self.tail)
            guard !Task.isCancelled else { return }
            self?.finish(rows: rows, stop: stop)
        }
    }

    /// The list went away: the window is dropped, unreported. The display link holds the meter until then.
    func cancel() {
        resting?.cancel()
        resting = nil
        link?.invalidate()
        link = nil
    }

    @objc private func frame(_ link: CADisplayLink) {
        if let last { frames.add(link.timestamp - last, expected: link.targetTimestamp - link.timestamp) }
        last = link.timestamp
        if let restingAt, let now = track?() { jump = max(jump, abs(now - restingAt)) }
    }

    private func finish(rows: Int, stop: PhonePerformance.Scroll.Stop?) {
        let seconds = CACurrentMediaTime() - started
        cancel()
        guard frames.count > 0 else { return }
        report(frames.event(list, rows: rows, seconds: seconds, stop: stop, jump: restingAt.map { _ in Int(jump.rounded()) }))
    }
}

/// The frames of one scroll: a frame is a hitch when it took longer than one and a half frame intervals.
struct ScrollFrames {
    private(set) var count = 0
    private var hitches = 0
    private var longest: TimeInterval = 0
    private var late: TimeInterval = 0
    /// The shortest frame interval asked of the display: its top refresh rate (ProMotion slows down when it can).
    private var shortestExpected: TimeInterval = .infinity

    mutating func add(_ interval: TimeInterval, expected: TimeInterval) {
        count += 1
        longest = max(longest, interval)
        shortestExpected = min(shortestExpected, expected)
        if interval > expected * 1.5 {
            hitches += 1
            late += interval - expected
        }
    }

    func event(_ list: PhonePerformance.Scroll.List, rows: Int, seconds: TimeInterval,
               stop: PhonePerformance.Scroll.Stop?, jump: Int?) -> PhonePerformance.Scroll {
        PhonePerformance.Scroll(list: list, rows: rows, frames: count, hitches: hitches,
                                     longestMs: Int((longest * 1000).rounded()),
                                     hitchMsPerS: seconds > 0 ? Int((late * 1000 / seconds).rounded()) : 0,
                                     fps: count > 0 ? Int((1 / shortestExpected).rounded()) : 0, stop: stop, jump: jump)
    }
}
