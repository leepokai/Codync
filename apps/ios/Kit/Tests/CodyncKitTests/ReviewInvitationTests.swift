import Foundation
import Testing
@testable import CodyncKit

private let reviewNow = Date(timeIntervalSince1970: 1_800_000_000)

@MainActor
private func tracker(_ suite: String) -> ReviewInvitation {
    ReviewInvitation(defaults: UserDefaults(suiteName: suite)!, capture: { _ in })
}

@Test func reviewBecomesEligibleAtTheFifthPrompt() {
    var history = ReviewHistory(prompts: 4)
    #expect(!history.eligible(at: reviewNow, version: "2.29.0"))
    history.prompts = 5
    #expect(history.eligible(at: reviewNow, version: "2.29.0"))
}

@Test func reviewCooldownSurvivesVersionsAndRequiresNewUse() {
    var history = ReviewHistory(prompts: 5)
    history.recordRequest(at: reviewNow, version: "2.29.0")
    history.prompts += 5
    #expect(!history.eligible(at: reviewNow.addingTimeInterval(120 * ReviewHistory.day - 1), version: "2.30.0"))
    let later = reviewNow.addingTimeInterval(120 * ReviewHistory.day)
    #expect(!history.eligible(at: later, version: "2.29.0"))
    #expect(history.eligible(at: later, version: "2.30.0"))
    history.prompts -= 1
    #expect(!history.eligible(at: later, version: "2.30.0"))
}

@Test func reviewAllowsAtMostTwoRequestsInRollingYear() {
    var history = ReviewHistory(prompts: 5)
    history.recordRequest(at: reviewNow, version: "2.29.0")
    history.prompts += 5
    history.recordRequest(at: reviewNow.addingTimeInterval(120 * ReviewHistory.day), version: "2.30.0")
    history.prompts += 5
    #expect(!history.eligible(at: reviewNow.addingTimeInterval(364 * ReviewHistory.day), version: "2.31.0"))
    #expect(history.eligible(at: reviewNow.addingTimeInterval(365 * ReviewHistory.day), version: "2.31.0"))
}

@Test func reviewRemembersVersionsAfterOldAttemptsExpire() {
    var history = ReviewHistory(prompts: 5)
    history.recordRequest(at: reviewNow, version: "2.29.0")
    history.prompts += 5
    let nextYear = reviewNow.addingTimeInterval(366 * ReviewHistory.day)
    history.recordRequest(at: nextYear, version: "2.30.0")
    history.prompts += 5
    #expect(!history.eligible(at: nextYear.addingTimeInterval(120 * ReviewHistory.day), version: "2.29.0"))
}

@MainActor
@Test func reviewOpensAtTheFifthPromptAcrossRelaunches() {
    let suite = UUID().uuidString
    defer { UserDefaults(suiteName: suite)!.removePersistentDomain(forName: suite) }
    let first = tracker(suite)
    for _ in 1...4 { first.sent(version: "2.29.0", now: reviewNow) }
    #expect(first.opportunity == nil)
    let relaunched = tracker(suite)
    #expect(relaunched.opportunity == nil)
    relaunched.sent(version: "2.29.0", now: reviewNow)
    #expect(relaunched.opportunity != nil)
}

@MainActor
@Test func reviewMissedRequestLeavesTheNextPromptToAsk() throws {
    let suite = UUID().uuidString
    defer { UserDefaults(suiteName: suite)!.removePersistentDomain(forName: suite) }
    let invitation = tracker(suite)
    for _ in 1...5 { invitation.sent(version: "2.29.0", now: reviewNow) }
    let missed = try #require(invitation.opportunity)
    invitation.sent(version: "2.29.0", now: reviewNow)
    let next = try #require(invitation.opportunity)
    #expect(next != missed)
    #expect(!invitation.request(missed, version: "2.29.0", now: reviewNow))
    #expect(invitation.request(next, version: "2.29.0", now: reviewNow))
    #expect(invitation.opportunity == nil)
}

@MainActor
@Test func reviewSkippedRequestWaitsForTheNextPrompt() throws {
    let suite = UUID().uuidString
    defer { UserDefaults(suiteName: suite)!.removePersistentDomain(forName: suite) }
    let invitation = tracker(suite)
    for _ in 1...5 { invitation.sent(version: "2.29.0", now: reviewNow) }
    let skipped = try #require(invitation.opportunity)
    invitation.skip(skipped)
    #expect(invitation.opportunity == nil)
    #expect(!invitation.request(skipped, version: "2.29.0", now: reviewNow))
    invitation.sent(version: "2.29.0", now: reviewNow)
    let next = try #require(invitation.opportunity)
    invitation.skip(skipped)
    #expect(invitation.opportunity == next, "skipping an older one leaves the new one")
}

@MainActor
@Test func reviewStaleRequestWaitsForTheNextPrompt() throws {
    let suite = UUID().uuidString
    defer { UserDefaults(suiteName: suite)!.removePersistentDomain(forName: suite) }
    let invitation = tracker(suite)
    for _ in 1...5 { invitation.sent(version: "2.29.0", now: reviewNow) }
    let stale = try #require(invitation.opportunity)
    let later = reviewNow.addingTimeInterval(300)
    #expect(!invitation.request(stale, version: "2.29.0", now: later))
    #expect(invitation.opportunity == nil)
    invitation.sent(version: "2.29.0", now: later)
    let next = try #require(invitation.opportunity)
    #expect(invitation.request(next, version: "2.29.0", now: later.addingTimeInterval(1)))
}

@MainActor
@Test func reviewRequestPersistsBeforeCallingStoreKitAndRespectsAnalyticsOptOut() throws {
    let suite = UUID().uuidString
    let defaults = UserDefaults(suiteName: suite)!
    defer { defaults.removePersistentDomain(forName: suite) }
    defaults.set(false, forKey: Analytics.enabledKey)
    let analytics = Analytics(defaults: defaults, upload: { _ in true })
    let invitation = ReviewInvitation(defaults: defaults, capture: { analytics.capture($0) })
    for _ in 1...5 { invitation.sent(version: "2.29.0", now: reviewNow) }
    let token = try #require(invitation.opportunity)
    #expect(invitation.request(token, version: "2.29.0", now: reviewNow))
    #expect(!invitation.request(token, version: "2.29.0", now: reviewNow))
    let history = try JSONDecoder().decode(ReviewHistory.self, from: #require(defaults.data(forKey: ReviewInvitation.storageKey)))
    #expect(history.requests.count == 1)
    #expect(!history.eligible(at: reviewNow, version: "2.30.0"))
    #expect(analytics.pending.isEmpty)
    invitation.sent(version: "2.29.0", now: reviewNow)
    #expect(invitation.opportunity == nil)
}

@MainActor
@Test func reviewKeepsTheCooldownOfAHistoryFromBeforePromptsCounted() {
    let suite = UUID().uuidString
    let defaults = UserDefaults(suiteName: suite)!
    defer { defaults.removePersistentDomain(forName: suite) }
    let asked = reviewNow.timeIntervalSinceReferenceDate
    let older = #"{"firstUse":\#(asked),"activeDays":["2026-10-1"],"completed":9,"sent":{},"#
        + #""requests":[{"date":\#(asked),"version":"2.38.0","completed":5}],"requestedVersions":["2.38.0"]}"#
    defaults.set(Data(older.utf8), forKey: ReviewInvitation.storageKey)
    let invitation = tracker(suite)
    let due = reviewNow.addingTimeInterval(120 * ReviewHistory.day)
    for _ in 1...5 { invitation.sent(version: "2.40.0", now: due.addingTimeInterval(-1)) }
    #expect(invitation.opportunity == nil)
    invitation.sent(version: "2.38.0", now: due)
    #expect(invitation.opportunity == nil)
    invitation.sent(version: "2.40.0", now: due)
    #expect(invitation.opportunity != nil)
}

@MainActor
@Test func reviewStaysOutOfTheWayOfE2EFlows() {
    let suite = UUID().uuidString
    let defaults = UserDefaults(suiteName: suite)!
    defer { defaults.removePersistentDomain(forName: suite) }
    defaults.set(true, forKey: ReviewInvitation.offKey)
    let invitation = tracker(suite)
    for _ in 1...5 { invitation.sent(version: "2.29.0", now: reviewNow) }
    #expect(invitation.opportunity == nil)
}

@Test func reviewAnalyticsContainsOnlyCountsAndFixedNames() {
    #expect(AnalyticsEvent.reviewRequested(prompts: 5).name == "review_requested")
    #expect(AnalyticsEvent.reviewRequested(prompts: 5).properties == ["prompts": .int(5), "policy": .string("v2")])
    #expect(AnalyticsEvent.reviewLinkOpened.properties == ["source": .string("settings")])
}
