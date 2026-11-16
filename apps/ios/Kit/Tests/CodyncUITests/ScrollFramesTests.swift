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
