import Foundation
import Testing
@testable import CodyncKit

@Test func activityErrorAndUnknownStatusNeverLookCompleted() {
    let error = BotActivityPresentation(status: "error", activity: "")
    #expect(error.phase == .failed)
    #expect(error.title == "Failed")
    #expect(error.symbol != "checkmark")
    #expect(error.progressStep == 4)
    let unknown = BotActivityPresentation(status: "new-host-status", activity: "")
    #expect(unknown.phase == .waiting)
    #expect(unknown.symbol != "checkmark")
    #expect(unknown.progressStep < 4)
}

@Test func staleActivityStopsPresentingLiveProgress() {
    for status in ["working", "needsInput"] {
        let stale = BotActivityPresentation(status: status, activity: "Old step", isStale: true)
        #expect(stale.phase == .stale)
        #expect(!stale.caption.contains("Old step"))
    }
    let done = BotActivityPresentation(status: "idle", activity: "Old step", isStale: true)
    #expect(done.phase == .completed)
    #expect(done.symbol == "checkmark")
}

@Test func activityCaptionIsTheStepWhileLive() {
    #expect(BotActivityPresentation(status: "needsInput", activity: "Choose a branch").caption == "Choose a branch")
    #expect(BotActivityPresentation(status: "working", activity: "").caption == "Working")
    #expect(BotActivityPresentation(status: "idle", activity: "Old step").caption == "Done")
}

@MainActor @Test func teamWidgetKeepsPicksAndFillsTheRestByUrgency() {
    let bots = Bot.widgetPreview  // review: needs you, build: working, docs: idle
    #expect(BotsTeamCard.slots(bots: bots, picks: []).map { $0?.id } == ["preview-review", "preview-build", "preview-docs", nil])
    let picked = BotsTeamCard.slots(bots: bots, picks: [nil, "preview-docs", nil, nil]).map { $0?.id }
    #expect(picked == ["preview-review", "preview-docs", "preview-build", nil])
    // A deleted pick falls back to automatic.
    #expect(BotsTeamCard.slots(bots: bots, picks: ["gone"]).first??.id == "preview-review")
}
