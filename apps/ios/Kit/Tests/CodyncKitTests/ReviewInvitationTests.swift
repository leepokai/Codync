import Foundation
import Testing
@testable import CodyncKit

private let reviewNow = Date(timeIntervalSince1970: 1_800_000_000)

private func eligibleHistory() -> ReviewHistory {
    ReviewHistory(firstUse: reviewNow.addingTimeInterval(-8 * ReviewHistory.day),
                  activeDays: ["2026-1-1", "2026-1-2", "2026-1-3"], completed: 5)
}

@Test func reviewRequiresAgeActiveDaysAndCompletedInteractions() {
    let now = reviewNow
    var history = eligibleHistory()
    #expect(history.eligible(at: now, version: "2.29.0"))
    history.firstUse = now.addingTimeInterval(-7 * ReviewHistory.day + 1)
    #expect(!history.eligible(at: now, version: "2.29.0"))
    history.firstUse = now.addingTimeInterval(-7 * ReviewHistory.day)
    #expect(history.eligible(at: now, version: "2.29.0"))
    history.activeDays.remove("2026-1-3")
    #expect(!history.eligible(at: now, version: "2.29.0"))
    history = eligibleHistory()
    history.completed = 4
    #expect(!history.eligible(at: now, version: "2.29.0"))
}

@Test func reviewCooldownSurvivesVersionsAndRequiresNewUse() {
    var history = eligibleHistory()
    history.recordRequest(at: reviewNow, version: "2.29.0")
    history.completed += 5
    #expect(!history.eligible(at: reviewNow.addingTimeInterval(120 * ReviewHistory.day - 1), version: "2.30.0"))
    let later = reviewNow.addingTimeInterval(120 * ReviewHistory.day)
    #expect(!history.eligible(at: later, version: "2.29.0"))
    #expect(history.eligible(at: later, version: "2.30.0"))
    history.completed -= 1
    #expect(!history.eligible(at: later, version: "2.30.0"))
}

@Test func reviewAllowsAtMostTwoRequestsInRollingYear() {
    var history = eligibleHistory()
    history.recordRequest(at: reviewNow, version: "2.29.0")
    history.completed += 5
    history.recordRequest(at: reviewNow.addingTimeInterval(120 * ReviewHistory.day), version: "2.30.0")
    history.completed += 5
    #expect(!history.eligible(at: reviewNow.addingTimeInterval(364 * ReviewHistory.day), version: "2.31.0"))
    #expect(history.eligible(at: reviewNow.addingTimeInterval(365 * ReviewHistory.day), version: "2.31.0"))
}

private func reviewEntries(nonce: String = "local-send", turn: Int64 = 1) -> [Entry] {
    var final = EntryData(text: "Done")
    final.final = true
    return [
        Entry(id: "u", seq: 1, botId: "b", rev: 1, kind: "user", turn: turn,
              data: EntryData(text: "Hello", status: "sent", clientNonce: nonce), createdAt: 1, updatedAt: 1),
        Entry(id: "a", seq: 2, botId: "b", rev: 2, kind: "agent", turn: turn, data: final, createdAt: 2, updatedAt: 2),
    ]
}

@Test func reviewIgnoresHistoricalOtherDeviceAndExpiredSends() {
    let entries = reviewEntries()
    #expect(ReviewCompletion.match(entries: entries, sent: [:], now: reviewNow) == nil)
    #expect(ReviewCompletion.match(entries: entries, sent: ["another-device": reviewNow], now: reviewNow) == nil)
    #expect(ReviewCompletion.match(entries: entries, sent: ["local-send": reviewNow.addingTimeInterval(-30 * ReviewHistory.day)], now: reviewNow) == nil)
    #expect(ReviewCompletion.match(entries: entries, sent: ["local-send": reviewNow], now: reviewNow) == ["local-send"])
}

@Test func reviewIgnoresPartialFailedCancelledAndRoutineTurns() {
    let sent = ["local-send": reviewNow]
    var entries = reviewEntries()
    entries[1].data.final = false
    #expect(ReviewCompletion.match(entries: entries, sent: sent, now: reviewNow) == nil)
    for status in ["failed", "cancelled", "queued"] {
        entries = reviewEntries()
        entries[0].data.status = status
        #expect(ReviewCompletion.match(entries: entries, sent: sent, now: reviewNow) == nil)
    }
    entries = reviewEntries()
    entries[0].data.routineId = "routine"
    #expect(ReviewCompletion.match(entries: entries, sent: sent, now: reviewNow) == nil)
    entries = reviewEntries()
    var notice = entries[1]
    notice.id = "stop"; notice.kind = "notice"; notice.data = EntryData(text: "Stopped.")
    entries.append(notice)
    #expect(ReviewCompletion.match(entries: entries, sent: sent, now: reviewNow) == nil)
}

@MainActor
@Test func reviewCountsTurnOnceAcrossReplaysAndRelaunch() throws {
    let suite = UUID().uuidString
    let defaults = UserDefaults(suiteName: suite)!
    defer { defaults.removePersistentDomain(forName: suite) }
    let tracker = ReviewInvitation(defaults: defaults, now: reviewNow, capture: { _ in })
    tracker.setActive(true, now: reviewNow)
    tracker.sent("local-send", now: reviewNow)
    tracker.viewed(chat: "c/b", entries: reviewEntries(), now: reviewNow)
    tracker.viewed(chat: "c/b", entries: reviewEntries(), now: reviewNow)
    let relaunched = ReviewInvitation(defaults: defaults, now: reviewNow, capture: { _ in })
    relaunched.setActive(true, now: reviewNow)
    relaunched.viewed(chat: "c/b", entries: reviewEntries(), now: reviewNow)
    let history = try JSONDecoder().decode(ReviewHistory.self, from: #require(defaults.data(forKey: ReviewInvitation.storageKey)))
    #expect(history.completed == 1)
}

@MainActor
@Test func reviewOnlyRequestsAfterReturnAndCancelsOnBackground() throws {
    let suite = UUID().uuidString
    let defaults = UserDefaults(suiteName: suite)!
    defer { defaults.removePersistentDomain(forName: suite) }
    try defaults.set(JSONEncoder().encode(eligibleHistory()), forKey: ReviewInvitation.storageKey)
    var events: [AnalyticsEvent] = []
    let tracker = ReviewInvitation(defaults: defaults, now: reviewNow, capture: { events.append($0) })
    tracker.setActive(true, now: reviewNow)
    tracker.sent("local-send", now: reviewNow)
    tracker.viewed(chat: "c/b", entries: reviewEntries(), now: reviewNow)
    #expect(tracker.opportunity == nil)
    tracker.returned(from: "c/b", version: "2.29.0", now: reviewNow.addingTimeInterval(59))
    #expect(tracker.opportunity == nil)
    tracker.sent("next", now: reviewNow)
    tracker.viewed(chat: "c/b", entries: reviewEntries(nonce: "next", turn: 2), now: reviewNow)
    tracker.returned(from: "c/b", version: "2.29.0", now: reviewNow.addingTimeInterval(60))
    let token = try #require(tracker.opportunity)
    tracker.setActive(false)
    #expect(!tracker.request(token, version: "2.29.0", now: reviewNow.addingTimeInterval(62)))
    #expect(events.last == .reviewCancelled(.background))
    tracker.setActive(true, now: reviewNow.addingTimeInterval(100))
    #expect(tracker.opportunity == nil)
}

@MainActor
@Test func reviewRequestPersistsBeforeCallingStoreKitAndRespectsAnalyticsOptOut() throws {
    let suite = UUID().uuidString
    let defaults = UserDefaults(suiteName: suite)!
    defer { defaults.removePersistentDomain(forName: suite) }
    defaults.set(false, forKey: Analytics.enabledKey)
    let analytics = Analytics(defaults: defaults, upload: { _ in true })
    try defaults.set(JSONEncoder().encode(eligibleHistory()), forKey: ReviewInvitation.storageKey)
    let tracker = ReviewInvitation(defaults: defaults, now: reviewNow, capture: { analytics.capture($0) })
    tracker.setActive(true, now: reviewNow)
    tracker.sent("local-send", now: reviewNow)
    tracker.viewed(chat: "c/b", entries: reviewEntries(), now: reviewNow)
    tracker.returned(from: "c/b", version: "2.29.0", now: reviewNow.addingTimeInterval(60))
    let token = try #require(tracker.opportunity)
    #expect(tracker.request(token, version: "2.29.0", now: reviewNow.addingTimeInterval(62)))
    #expect(!tracker.request(token, version: "2.29.0", now: reviewNow.addingTimeInterval(63)))
    let history = try JSONDecoder().decode(ReviewHistory.self, from: #require(defaults.data(forKey: ReviewInvitation.storageKey)))
    #expect(history.requests.count == 1)
    #expect(!history.eligible(at: reviewNow.addingTimeInterval(63), version: "2.30.0"))
    #expect(analytics.pending.isEmpty)
}

@Test func reviewAnalyticsContainsOnlyCountsAndFixedNames() {
    #expect(AnalyticsEvent.reviewRequested(completed: 8).name == "review_requested")
    #expect(AnalyticsEvent.reviewRequested(completed: 8).properties == [
        "completed_interactions": .int(8), "policy": .string("v1"), "trigger": .string("return_to_bots"),
    ])
    #expect(AnalyticsEvent.reviewCancelled(.interaction).properties["reason"] == .string("interaction"))
    #expect(AnalyticsEvent.reviewLinkOpened.properties == ["source": .string("settings")])
}

@MainActor
@Test func reviewDoesNotCountResultsBehindNestedPresentations() throws {
    let suite = UUID().uuidString
    let defaults = UserDefaults(suiteName: suite)!
    defer { defaults.removePersistentDomain(forName: suite) }
    let tracker = ReviewInvitation(defaults: defaults, now: reviewNow, capture: { _ in })
    tracker.setActive(true, now: reviewNow)
    tracker.sent("local-send", now: reviewNow)
    let sheet = UUID(), picker = UUID()
    tracker.setInterrupted(sheet, active: true)
    tracker.setInterrupted(picker, active: true)
    tracker.setInterrupted(sheet, active: false)
    tracker.viewed(chat: "c/b", entries: reviewEntries(), now: reviewNow)
    var history = try JSONDecoder().decode(ReviewHistory.self, from: #require(defaults.data(forKey: ReviewInvitation.storageKey)))
    #expect(history.completed == 0)
    #expect(history.sent["local-send"] != nil)
    tracker.setInterrupted(picker, active: false)
    tracker.viewed(chat: "c/b", entries: reviewEntries(), now: reviewNow)
    history = try JSONDecoder().decode(ReviewHistory.self, from: #require(defaults.data(forKey: ReviewInvitation.storageKey)))
    #expect(history.completed == 1)
}

@MainActor
@Test func reviewRemovesExpiredNoncesOnNextLaunch() throws {
    let suite = UUID().uuidString
    let defaults = UserDefaults(suiteName: suite)!
    defer { defaults.removePersistentDomain(forName: suite) }
    var history = eligibleHistory()
    history.sent = ["old": reviewNow.addingTimeInterval(-30 * ReviewHistory.day), "recent": reviewNow]
    try defaults.set(JSONEncoder().encode(history), forKey: ReviewInvitation.storageKey)
    _ = ReviewInvitation(defaults: defaults, now: reviewNow, capture: { _ in })
    let saved = try JSONDecoder().decode(ReviewHistory.self, from: #require(defaults.data(forKey: ReviewInvitation.storageKey)))
    #expect(saved.sent == ["recent": reviewNow])
}

@Test func reviewCountsSeveralMessagesInOneTurnTogetherAndExcludesCalls() {
    var entries = reviewEntries()
    var second = entries[0]
    second.id = "u2"; second.seq = 2; second.data.clientNonce = "second"
    entries.insert(second, at: 1)
    #expect(ReviewCompletion.match(entries: entries, sent: ["local-send": reviewNow, "second": reviewNow], now: reviewNow) == ["local-send", "second"])
    entries = reviewEntries()
    entries[0].data.call = "voice"
    #expect(ReviewCompletion.match(entries: entries, sent: ["local-send": reviewNow], now: reviewNow) == nil)
}

@Test func reviewRemembersVersionsAfterOldAttemptsExpire() {
    var history = eligibleHistory()
    history.recordRequest(at: reviewNow, version: "2.29.0")
    history.completed += 5
    let nextYear = reviewNow.addingTimeInterval(366 * ReviewHistory.day)
    history.recordRequest(at: nextYear, version: "2.30.0")
    history.completed += 5
    #expect(!history.eligible(at: nextYear.addingTimeInterval(120 * ReviewHistory.day), version: "2.29.0"))
}
