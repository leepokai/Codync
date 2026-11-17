import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A store on `remote`, subscribed and counting its roster callbacks.
@MainActor
private final class Rig {
    let remote = FakeRemote(.ready(.direct))
    let store: BotStore
    var rosterChanges = 0
    private let suite = "RosterCurrentTests.\(UUID().uuidString)"

    init() {
        let remote = remote
        store = BotStore(computer: Computer(id: UUID().uuidString, name: "Mac", signKey: "unused", boxKey: "unused"),
                         storage: SharedStore.Context(accountID: nil, suite: suite)) { _ in remote }
        store.onRosterChanged = { [weak self] in self?.rosterChanges += 1 }
    }

    func start() async {
        store.setActive(true)
        #expect(await until { await remote.subscribed })
    }

    func hello(rev: Int, catchUp: Int?) async {
        await remote.emit(#"{"type":"hello","hostId":"h1","rev":\#(rev)\#(catchUp.map { #","catchUp":\#($0)"# } ?? "")}"#)
        #expect(await until { store.connection == .online && store.catchUpRev == Int64(rev) })
    }

    /// A bot event from the stream, applied.
    func bot(rev: Int) async {
        await remote.emit(botEvent("b\(rev)", name: "Bot", rev: rev))
        #expect(await until { store.bots["b\(rev)"] != nil })
    }

    /// Long enough for the store to have handled what was sent, when nothing visible should come of it.
    func settle() async { try? await Task.sleep(for: .milliseconds(50)) }

    deinit { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
}

@MainActor @Test func theRosterIsCurrentOnlyOnceThisStreamsCatchUpEnded() async {
    let rig = Rig()
    defer { rig.store.retire() }
    #expect(!rig.store.rosterIsCurrent)
    await rig.start()
    await rig.hello(rev: 2, catchUp: 2)
    #expect(!rig.store.rosterIsCurrent, "hello only names what's coming")
    await rig.bot(rev: 1)
    #expect(!rig.store.rosterIsCurrent)
    let before = rig.rosterChanges
    await rig.bot(rev: 2)
    #expect(await until { rig.store.rosterIsCurrent })
    #expect(rig.rosterChanges == before + 2, "the bot, then the end of the catch-up")

    // Backgrounded and back in the same process: still `.online`, rev unchanged, but this stream has caught up on nothing.
    rig.store.setActive(false)
    #expect(!rig.store.rosterIsCurrent)
    #expect(await until { await !rig.remote.subscribed })
    await rig.start()
    #expect(rig.store.connection == .online)
    #expect(!rig.store.rosterIsCurrent)
    await rig.hello(rev: 3, catchUp: 1)
    #expect(!rig.store.rosterIsCurrent)
    await rig.bot(rev: 3)
    #expect(await until { rig.store.rosterIsCurrent })

    // Live events after the catch-up don't count against the next one.
    await rig.bot(rev: 4)
    #expect(rig.store.rosterIsCurrent)
}

@MainActor @Test func theRosterStopsBeingCurrentAsSoonAsItsStreamStops() async {
    let rig = Rig()
    defer { rig.store.retire() }
    await rig.start()
    await rig.hello(rev: 0, catchUp: 0)
    #expect(await until { rig.store.rosterIsCurrent })

    // The link drops: the header keeps saying online for the grace period, the roster is no longer current.
    let subscriptions = await rig.remote.eventSubscriptionCount
    await rig.remote.set(.connecting)
    #expect(await until { !rig.store.rosterIsCurrent })
    #expect(rig.store.connection == .online)
    await rig.remote.set(.ready(.direct))
    #expect(await until { await rig.remote.eventSubscriptionCount > subscriptions })
    await rig.hello(rev: 0, catchUp: 0)
    #expect(await until { rig.store.rosterIsCurrent })

    // The stream ends with the link up: not current during the wait for the next one.
    await rig.remote.endEvents()
    #expect(await until { !rig.store.rosterIsCurrent })
    #expect(rig.store.connection == .online)
    #expect(await until { await rig.remote.subscribed })
    await rig.settle()
    #expect(!rig.store.rosterIsCurrent)
}

@MainActor @Test func aHostRevAheadOfEveryEventStillEndsTheCatchUp() async {
    // A write no event carries (a memory file, a prune): the count says when it's over, the rev never does.
    let rig = Rig()
    defer { rig.store.retire() }
    await rig.start()
    await rig.hello(rev: 9, catchUp: 1)
    let before = rig.rosterChanges
    await rig.bot(rev: 1)
    #expect(await until { rig.store.rosterIsCurrent })
    #expect(rig.store.rev < rig.store.catchUpRev)
    #expect(rig.rosterChanges > before)
}

@MainActor @Test func nothingToCatchUpOnIsCurrentAtHello() async {
    let rig = Rig()
    defer { rig.store.retire() }
    await rig.start()
    await rig.hello(rev: 9, catchUp: 0)
    #expect(await until { rig.store.rosterIsCurrent })
    #expect(rig.rosterChanges > 0)
}

@MainActor @Test func anEventThisAppCantReadStillCountsTowardsTheCatchUp() async {
    let rig = Rig()
    defer { rig.store.retire() }
    await rig.start()
    await rig.hello(rev: 9, catchUp: 2)
    await rig.bot(rev: 1)
    #expect(!rig.store.rosterIsCurrent)
    await rig.remote.emit(#"{"type":"from-the-future"}"#)
    #expect(await until { rig.store.rosterIsCurrent })
}

@MainActor @Test func aRewindOnTheLastCatchUpEventWaitsForTheNextStream() async {
    let rig = Rig()
    defer { rig.store.retire() }
    await rig.start()
    await rig.hello(rev: 9, catchUp: 1)
    await rig.remote.emit(#"{"type":"bot","bot":{"id":1}}"#)
    #expect(await until { rig.store.rewound })
    await rig.settle()
    #expect(!rig.store.rosterIsCurrent)
}

@MainActor @Test func aHostBeforeTheCountIsCurrentAtHello() async {
    // It sends no alert totals either, so there is nothing for its roster to replace.
    let rig = Rig()
    defer { rig.store.retire() }
    await rig.start()
    await rig.hello(rev: 2, catchUp: nil)
    #expect(await until { rig.store.rosterIsCurrent })
}

@MainActor @Test func accountStoreNamesTheComputersStillBehind() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let computer = randomComputer("Desk")
    storage.computers = [computer]
    let remote = FakeRemote(.ready(.direct))
    let accounts = AccountStore(storage: storage, cloud: nil) { computer in
        BotStore(computer: computer, storage: storage) { _ in remote }
    }
    defer { accounts.retire() }
    #expect(accounts.computersBehind == [computer.id])
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":0,"catchUp":0}"#)
    #expect(await until { accounts.computersBehind.isEmpty })
}
