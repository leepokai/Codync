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

// The Codync row on a computer's card in Computer management.

@Test func theCodyncRowOffersANewerReleaseOrSaysItsCurrent() {
    let line = { (latest: String?, offer: HostUpdateOffer) in
        HostVersionLine.make(version: "2.28.0", latest: latest, mismatch: nil, offer: offer, host: "Mac", os: "macos")
    }
    #expect(line("2.28.0", .available(last: nil)) == HostVersionLine(version: "2.28.0", detail: ["Up to date"]))
    #expect(line("2.27.0", .available(last: nil)).detail == ["Up to date"])
    // A computer that doesn't know the latest release makes no claim.
    #expect(line(nil, .available(last: nil)) == HostVersionLine(version: "2.28.0", detail: []))
    #expect(line("2.29.0", .available(last: nil))
        == HostVersionLine(version: "2.28.0", detail: ["Version 2.29.0 is available."], accessory: .button("Update")))
    // Not from here: it says how, on that computer.
    #expect(line("2.29.0", .unavailable)
        == HostVersionLine(version: "2.28.0", detail: ["Version 2.29.0 is available.", "Open Codync on that Mac and choose Check for Updates."]))
    #expect(line("2.29.0", .updating)
        == HostVersionLine(version: "2.28.0", detail: ["Updating Codync on Mac. This takes a few minutes."], accessory: .updating))
}

@Test func aComputerTooOldOrAnUpdateThatDidntTakeIsAProblem() {
    let behind = VersionMismatch.updateHost(version: "2.20.0", minimum: "2.26.0")
    let old = HostVersionLine.make(version: "2.20.0", latest: nil, mismatch: behind, offer: .available(last: nil), host: "Mac", os: "linux")
    #expect(old == HostVersionLine(version: "2.20.0", detail: ["This app needs 2.26.0 or newer."], warns: true, accessory: .button("Update")))
    let manual = HostVersionLine.make(version: "2.20.0", latest: "2.29.0", mismatch: behind, offer: .unavailable, host: "Mac", os: "linux")
    #expect(manual.detail == ["This app needs 2.26.0 or newer.", "Run codync-host update there."] && manual.warns)
    let failed = HostVersionLine.make(version: "2.28.0", latest: "2.29.0", mismatch: nil,
                                      offer: .available(last: .failed("A bot is working on this computer.")), host: "Mac", os: "macos")
    #expect(failed == HostVersionLine(version: "2.28.0", detail: ["A bot is working on this computer."], warns: true,
                                      accessory: .button("Try again")))
    // Asked, but the computer never came back updated.
    let stalled = HostVersionLine.make(version: "2.28.0", latest: "2.29.0", mismatch: nil,
                                       offer: .available(last: .requested(at: .now)), host: "Mac", os: "macos")
    #expect(stalled == HostVersionLine(version: "2.28.0", detail: ["Mac didn't finish updating. Try again, or update Codync on that computer."],
                                       warns: true, accessory: .button("Try again")))
}

@MainActor @Test func theCodyncRowComparesWithTheLatestReleaseTheComputerFound() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.relay))
    await fake.setHello(helloJSON(version: "2.28.0", canUpdate: true, latestVersion: "2.30.0"))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage, appVersion: VersionMismatch.minHost) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { store.hello != nil })
    #expect(store.hostVersionLine()
        == HostVersionLine(version: "2.28.0", detail: ["Version 2.30.0 is available."], accessory: .button("Update")))
    // Asked, the row shows it under way until the wait is over, then offers it again.
    store.requestHostUpdate()
    #expect(store.hostVersionLine()?.accessory == .updating)
    let ends = try #require(store.hostUpdateWaitEnds)
    #expect(store.hostVersionLine(now: ends)?.accessory == .button("Try again"))
}
