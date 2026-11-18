import Foundation
import Testing
@testable import CodyncKit

private func storage(paired: [ComputerID], unread: [(ComputerID, Int)]) throws -> (SharedStore.Context, String) {
    let suite = "IconBadgeTests.\(UUID().uuidString)"
    let store = SharedStore.Context(accountID: nil, suite: suite)
    store.computers = paired.map { Computer(id: $0, name: $0, signKey: "unused") }
    store.bots = try unread.enumerated().map { index, row in
        let bot = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b\#(index)","unread":\#(row.1)}"#.utf8))
        return BotSnapshot(computerId: row.0, bot: bot)
    }
    return (store, suite)
}

private func push(_ totals: [ComputerID: Int], into store: SharedStore.Context) {
    for (computerId, unread) in totals { store.setPushedUnread(unread, for: computerId) }
}

@Test func anAlertsTotalStandsInForItsComputersSnapshotAndKeepsEarlierAlerts() throws {
    let (store, suite) = try storage(paired: ["a", "b", "c"], unread: [("a", 5), ("b", 0), ("c", 3)])
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    #expect(IconBadge.alerted(2, from: "a", in: store) == 5)
    #expect(IconBadge.alerted(1, from: "b", in: store) == 6)
    #expect(store.pushedUnread == ["a": 2, "b": 1])
}

@Test func aRemovedComputersAlertCountsNoMore() throws {
    let (store, suite) = try storage(paired: ["a"], unread: [("a", 1)])
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    push(["gone": 4], into: store)
    #expect(IconBadge.alerted(7, from: "gone", in: store) == nil)
    #expect(store.pushedUnread == ["gone": 4], "nothing from a computer no longer here is kept")
    #expect(IconBadge.alerted(2, from: "a", in: store) == 2)
    #expect(store.pushedUnread == ["a": 2], "an earlier alert of a removed computer goes with the next one")
}

@Test func onlyComputersStillBehindKeepTheirAlerts() throws {
    let (store, suite) = try storage(paired: ["a", "b", "c"], unread: [("a", 5), ("b", 0), ("c", 3)])
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    push(["a": 2, "b": 1, "gone": 4], into: store)
    // A's roster caught up: its live 5 replaces its alert. B, still unreachable, keeps its alert's 1. C sent none.
    #expect(IconBadge.settled(behind: ["b", "c"], in: store) == 9)
    #expect(store.pushedUnread == ["b": 1])
    #expect(IconBadge.settled(behind: [], in: store) == 8)
    #expect(store.pushedUnread.isEmpty)
}

@Test func eachComputersTotalIsItsOwnKeySoWritersNeverClobberEachOther() throws {
    let (app, suite) = try storage(paired: ["a", "b"], unread: [])
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    // The extension, in its own process, sets only B's; the app's settle drops only A's.
    #expect(IconBadge.alerted(3, from: "b", in: SharedStore.Context(accountID: nil, suite: suite)) == 3)
    push(["a": 1], into: app)
    #expect(IconBadge.settled(behind: ["b"], in: app) == 3)
    #expect(app.pushedUnread == ["b": 3])
}

@Test func aComputerCannotOverflowOrLowerTheBadge() throws {
    let (store, suite) = try storage(paired: ["a", "b"], unread: [("b", -3)])
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let badge = IconBadge.alerted(.max, from: "a", in: store)
    #expect(badge == 99_999)
    push(["a": .max, "b": .max], into: store)
    #expect(IconBadge.settled(behind: ["a", "b"], in: store) == 199_998, "what's saved already is bounded too")
    #expect(IconBadge.alerted(-2, from: "a", in: store) == 99_999)
}

@Test func theIconShowsNoCountWhileTheSwitchIsOff() {
    #expect(IconBadge.shown(4, enabled: false) == 0)
    #expect(IconBadge.shown(4, enabled: true) == 4)
}
