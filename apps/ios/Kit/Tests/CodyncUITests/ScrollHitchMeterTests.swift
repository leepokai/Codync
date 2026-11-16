import Testing
@testable import CodyncKit
@testable import CodyncUI

private let at120: Double = 1.0 / 120

@Test func aSteadyScrollHasNoHitches() {
    var frames = ScrollFrames()
    for _ in 0..<120 { frames.add(at120, expected: at120) }
    let scroll = frames.event(.bots, rows: 60, seconds: 1, stop: nil, jump: nil)
    #expect(scroll == .init(list: .bots, rows: 60, frames: 120, hitches: 0, longestMs: 8, hitchMsPerSecond: 0, fps: 120))
}

@Test func aLateFrameIsAHitchAndCountsOnlyItsLateTime() {
    var frames = ScrollFrames()
    for _ in 0..<58 { frames.add(at120, expected: at120) }
    // Exactly one and a half intervals is still on time; a frame shown four intervals late is a 25 ms hitch.
    frames.add(at120 * 1.5, expected: at120)
    frames.add(at120 * 4, expected: at120)
    let scroll = frames.event(.chat, rows: 320, seconds: 0.5, stop: .end, jump: 0)
    #expect(scroll.frames == 60)
    #expect(scroll.hitches == 1)
    #expect(scroll.longestMs == 33)
    #expect(scroll.hitchMsPerSecond == 50)
}

@Test func theTopRefreshRateIsTheShortestIntervalAsked() {
    // ProMotion drops to 60 while the scroll slows down; the display still runs up to 120.
    var frames = ScrollFrames()
    frames.add(at120, expected: at120)
    frames.add(1.0 / 60, expected: 1.0 / 60)
    #expect(frames.event(.bots, rows: 1, seconds: 1, stop: nil, jump: nil).fps == 120)
}

/// One drag, held for `drag`, then `ScrollHitchMeter.tail` and a little more for it to report.
@MainActor private func scroll(_ meter: ScrollHitchMeter, drag: Duration = .milliseconds(200), rows: Int = 40) async throws {
    meter.begin()
    try await Task.sleep(for: drag)
    meter.rest(rows: rows, stop: .end)
    try await Task.sleep(for: ScrollHitchMeter.tail + .milliseconds(200))
}

@MainActor @Test func aScrollReportsOnceWithItsFrames() async throws {
    var reports: [AnalyticsEvent.ScrollHitches] = []
    let meter = ScrollHitchMeter(.chat) { reports.append($0) }
    try await scroll(meter)
    #expect(reports.count == 1)
    let scroll = try #require(reports.first)
    #expect(scroll.list == .chat && scroll.rows == 40 && scroll.stop == .end && scroll.jump == nil)
    // Half a second of display link at 30 to 120 Hz, with no frame stuck for more than a second.
    #expect((5...70).contains(scroll.frames))
    #expect((30...120).contains(scroll.fps))
    #expect(scroll.hitches <= scroll.frames && scroll.longestMs < 1000 && scroll.hitchMsPerSecond < 1000)
}

@MainActor @Test func aDragDuringTheTailContinuesTheSameScroll() async throws {
    var reports: [AnalyticsEvent.ScrollHitches] = []
    let meter = ScrollHitchMeter(.bots) { reports.append($0) }
    meter.begin()
    try await Task.sleep(for: .milliseconds(100))
    meter.rest(rows: 40)
    try await Task.sleep(for: .milliseconds(100))
    try await scroll(meter)
    #expect(reports.count == 1)
}

@MainActor @Test func aListThatGoesAwayReportsNothing() async throws {
    var reports: [AnalyticsEvent.ScrollHitches] = []
    let meter = ScrollHitchMeter(.bots) { reports.append($0) }
    meter.begin()
    meter.rest(rows: 40)
    meter.cancel()
    try await Task.sleep(for: ScrollHitchMeter.tail + .milliseconds(200))
    #expect(reports.isEmpty)
}

@MainActor @Test func theSimulatorSendsNothing() async throws {
    try await scroll(ScrollHitchMeter(.bots))
    #expect(!Analytics.shared.pending.contains { $0.event == "scroll_hitches" })
}
