import Foundation
import Testing
@testable import CodyncKit

@Test func activityErrorAndUnknownStatusNeverLookCompleted() {
    let error = BotActivityPresentation(status: "error", activity: "", startedAt: .now)
    #expect(error.phase == .failed)
    #expect(error.symbol != "checkmark")
    #expect(!error.showsTimer)
    let unknown = BotActivityPresentation(status: "new-host-status", activity: "", startedAt: .now)
    #expect(unknown.phase == .waiting)
    #expect(unknown.symbol != "checkmark")
}

@Test func staleActivityStopsPresentingLiveProgress() {
    for status in ["working", "needsInput"] {
        let stale = BotActivityPresentation(status: status, activity: "Old step", startedAt: .now, isStale: true)
        #expect(stale.phase == .stale)
        #expect(!stale.showsTimer)
        #expect(!stale.detail.contains("Old step"))
    }
    let done = BotActivityPresentation(status: "idle", activity: "Old step", startedAt: .now, isStale: true)
    #expect(done.phase == .completed)
    #expect(!done.showsTimer)
    #expect(done.symbol == "checkmark")
}

@Test func activityTimerOnlyRunsDuringWork() {
    #expect(BotActivityPresentation(status: "working", activity: "", startedAt: .now).showsTimer)
    #expect(!BotActivityPresentation(status: "working", activity: "", startedAt: nil).showsTimer)
    let input = BotActivityPresentation(status: "needsInput", activity: "Choose a branch", startedAt: .now)
    #expect(input.detail == "Choose a branch")
    #expect(!input.showsTimer)
}
