import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// The real iPhone store and channel against a real host, which logs what the phone sends: an open's
/// first milestones land in its `phone-performance.jsonl`. Driven by `host/tests/phone_performance_e2e.rs`,
/// which runs only when asked. `TEST_RUNNER_CODYNC_PERFORMANCE_DIR=<dir>` holds the host's pairing URL,
/// and `TEST_RUNNER_CODYNC_RECORD_PERFORMANCE=1` lets the simulator send.
@MainActor @Test(.enabled(if: performanceDir != nil), .timeLimit(.minutes(5)))
func aPhoneOpenSendsItsTimingToTheHost() async throws {
    let dir = try #require(performanceDir)
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let identity = DeviceIdentity(signing: .init(), push: .init())
    // A debug host links for Codync Dev; the test bundle reads links as the app it stands for.
    var link = try #require(URLComponents(string: try await handoff(dir, "pairing")))
    link.scheme = AppEnvironment.current.urlScheme
    let pairing = try #require(link.url.flatMap(Pairing.init(url:)))
    let computer = try await HostConnector.pair(pairing, identity: identity, deviceName: "Performance test", platform: "ios",
                                                certificate: nil, claim: nil).computer
    let timing = OpenTiming()
    timing.beginCold()
    let store = BotStore(computer: computer, storage: storage) { current in
        try await HostConnector.connect(current, identity: identity)
    }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await wait { store.caughtUp && store.connection == .online })
    // Delivery is fire-and-forget: let the sends reach the host before the test ends.
    try await Task.sleep(for: .milliseconds(500))
}

private let performanceDir = ProcessInfo.processInfo.environment["CODYNC_PERFORMANCE_DIR"].map(URL.init(fileURLWithPath:))

private func handoff(_ dir: URL, _ name: String) async throws -> String {
    for _ in 0..<600 {
        if let text = try? String(contentsOf: dir.appendingPathComponent(name), encoding: .utf8) { return text }
        try await Task.sleep(for: .milliseconds(100))
    }
    throw HandoffError.missing(name)
}

private enum HandoffError: Error { case missing(String) }

@MainActor private func wait(_ condition: @MainActor () async -> Bool) async -> Bool {
    for _ in 0..<200 {
        if await condition() { return true }
        try? await Task.sleep(for: .milliseconds(50))
    }
    return false
}
