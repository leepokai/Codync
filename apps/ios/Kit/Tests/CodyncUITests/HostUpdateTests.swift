import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

// Updating a computer that's behind this app, from here (BotStore+HostUpdate).

@MainActor @Test func aComputerThatsBehindUpdatesWhenAsked() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.relay))
    await fake.setHello(helloJSON(version: "2.2.0", canUpdate: true))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage, appVersion: VersionMismatch.minHost) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { store.mismatch != nil })
    #expect(store.hostUpdateOffer() == .available(last: nil))

    store.requestHostUpdate()
    #expect(store.hostUpdateOffer() == .updating)
    #expect(await until { await fake.calls.contains("installHostUpdate") })
    // It never came back updated: offered again, saying so.
    guard case let .requested(at) = store.hostUpdate else { Issue.record("not requested"); return }
    #expect(store.hostUpdateOffer(now: at + BotStore.hostUpdateWait) == .available(last: .requested(at: at)))

    await fake.fail("installHostUpdate", times: 1)
    store.requestHostUpdate()
    let failed = HostUpdate.failed(HostError.unreachable.localizedDescription)
    #expect(await until { store.hostUpdate == failed })
    #expect(store.hostUpdateOffer() == .available(last: failed))

    // The computer comes back updated: nothing is left of the request.
    await fake.setHello(helloJSON(version: VersionMismatch.minHost, canUpdate: true))
    await fake.set(.connecting)
    await fake.set(.ready(.relay))
    #expect(await until { store.mismatch == nil })
    #expect(store.hostUpdate == nil)
    // A host that can't update on request offers nothing.
    store.hello?.canUpdate = nil
    #expect(store.hostUpdateOffer() == .unavailable)
}
