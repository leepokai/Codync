import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A screen pauses once nobody has touched the app for the whole idle limit since it went live, not a moment before.
@MainActor @Test func screenPausesOnlyAfterTheIdleLimit() {
    let live = Date(timeIntervalSince1970: 1_000)
    #expect(ScreenPause.idleLimit == .seconds(5 * 60))
    #expect(ScreenPause.idleLeft(lastTouch: live, start: live, now: live) == ScreenPause.idleLimit)
    #expect(ScreenPause.idleLeft(lastTouch: live, start: live, now: live + 299) == .seconds(1))
    #expect(ScreenPause.idleLeft(lastTouch: live, start: live, now: live + 300) == nil)
    #expect(ScreenPause.idleLeft(lastTouch: live, start: live, now: live + 3_600) == nil)
    // Touched long before it went live: the count starts with the picture.
    #expect(ScreenPause.idleLeft(lastTouch: live - 3_600, start: live, now: live) == ScreenPause.idleLimit)
    #expect(ScreenPause.idleLeft(lastTouch: live - 3_600, start: live, now: live + 240) == .seconds(60))
    // A touch while live starts the count over.
    #expect(ScreenPause.idleLeft(lastTouch: live + 180, start: live, now: live + 240) == .seconds(240))
}

/// Not asked for yet, then paused: one play button with the other apps' words; while watching the picture shows.
@MainActor @Test func screenOffersStartThenContinue() {
    #expect(ScreenPause.State.start.prompt == "Tap to see the screen")
    #expect(ScreenPause.State.paused.prompt == "Paused. Tap to continue.")
    #expect(ScreenPause.State.watching.prompt == nil)

    let pause = ScreenPause(.botComputer)
    #expect(ScreenNotice(pause)?.icon == "play.circle")
    pause.play()
    #expect(pause.state == .watching && ScreenNotice(pause) == nil)
    pause.reset()
    #expect(pause.state == .start)

    #expect(AnalyticsEvent.remoteScreenPaused(.botComputer).name == "remote_screen_paused")
    #expect(AnalyticsEvent.remoteScreenPaused(.remoteScreen).properties == ["screen": .string("remote_screen")])
}
