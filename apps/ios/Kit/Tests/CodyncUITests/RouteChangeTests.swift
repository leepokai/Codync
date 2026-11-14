import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A route picked in Settings takes effect on the next channel: each connect gets the computer as it
/// is now, not as it was when its store opened.
@MainActor @Test func newRouteReachesTheNextChannel() async throws {
    let suite = "RouteChange.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: "account", suite: suite)
    let key = Data(repeating: 3, count: 32)
    let computer = Computer(id: RelayCrypto.computerId(signKey: key), name: "Desk", signKey: key.base64URL)
    storage.computers = [computer]
    var routes: [ConnectionRoute?] = []
    let accounts = AccountStore(storage: storage, cloud: nil) { computer in
        BotStore(computer: computer, storage: storage) { current in
            routes.append(current.route)
            return FakeRemote(.connecting)
        }
    }
    defer { accounts.retire() }
    for _ in 0..<100 where routes.isEmpty { try await Task.sleep(for: .milliseconds(10)) }

    accounts.setRoute(computer.id, .directOnly)
    for _ in 0..<100 where routes.count < 2 { try await Task.sleep(for: .milliseconds(10)) }
    #expect(routes == [nil, .directOnly])
}
