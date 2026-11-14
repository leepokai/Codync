import Foundation
import os
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// Answers `/v1/computers` with the subclass's `computers(for:)` and every other request with `{}`.
private class CloudStub: URLProtocol {
    /// The listed computers' JSON objects, comma-separated; asked for every request.
    class func computers(for request: URLRequest) -> String { "" }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() {}

    override func startLoading() {
        let computers = Self.computers(for: request)
        let json = request.url?.path.hasSuffix("/v1/computers") == true ? #"{"computers":[\#(computers)]}"# : "{}"
        let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
}

/// A cloud client whose requests `stub` answers.
private func stubbedCloud(_ stub: CloudStub.Type, token: @escaping @Sendable () async throws -> String = { "token" }) -> CloudClient {
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [stub]
    return CloudClient(baseURL: URL(string: "https://cloud.test")!, identity: DeviceIdentity(signing: .init(), push: .init()),
                       session: URLSession(configuration: config), token: token)
}

/// The account lists an offline "Desk" last seen at 200 and an online "Desk" claimed at `claimedAt`.
private final class OlderCopyCloudStub: CloudStub {
    static let claimedAt = OSAllocatedUnfairLock(initialState: 0)

    override class func computers(for request: URLRequest) -> String {
        #"{"computerId":"old","name":"Desk","signKey":"a","online":false,"lastSeenAt":200},"#
            + #"{"computerId":"new","name":"Desk","signKey":"b","online":true,"claimedAt":\#(claimedAt.withLock { $0 })}"#
    }
}

/// A same-named offline computer is an older copy only if it was last seen before the online one
/// joined the account; seen after, it ran beside it and is another machine.
@MainActor @Test func olderCopyNeedsToBeLastSeenBeforeTheCurrentOneJoined() async throws {
    let suite = "OlderCopy.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let accounts = AccountStore(storage: SharedStore.Context(accountID: "account", suite: suite), cloud: stubbedCloud(OlderCopyCloudStub.self))
    defer { accounts.retire() }

    OlderCopyCloudStub.claimedAt.withLock { $0 = 100 }
    await accounts.refreshCloud()
    let old = try #require(accounts.cloudComputers.first { $0.computerId == "old" })
    #expect(!accounts.isOlderCopy(old))

    OlderCopyCloudStub.claimedAt.withLock { $0 = 300 }
    await accounts.refreshCloud()
    #expect(accounts.isOlderCopy(old))
}

private let currentKey = Data(repeating: 5, count: 32)

/// The account lists only the online "Desk", claimed at `claimedAt`.
private final class StaleCopyCloudStub: CloudStub {
    static let claimedAt = OSAllocatedUnfairLock(initialState: 0)

    override class func computers(for request: URLRequest) -> String {
        #"{"computerId":"\#(RelayCrypto.computerId(signKey: currentKey))","name":"Desk","signKey":"\#(currentKey.base64URL)","online":true,"claimedAt":\#(claimedAt.withLock { $0 })}"#
    }
}

/// A computer paired here and gone offline is stale beside an online one with its name only if the relay
/// last saw it before that one joined the account; seen after, it's another machine still to come back.
@MainActor @Test func offlineComputerIsStaleOnlyIfLastSeenBeforeTheCurrentOneJoined() async throws {
    let suite = "StaleCopy.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: "account", suite: suite)
    let oldKey = Data(repeating: 4, count: 32)
    let old = Computer(id: RelayCrypto.computerId(signKey: oldKey), name: "Desk", signKey: oldKey.base64URL)
    storage.computers = [old]
    let accounts = AccountStore(storage: storage, cloud: stubbedCloud(StaleCopyCloudStub.self)) { computer in
        BotStore(computer: computer, storage: storage) { _ in FakeRemote(.connecting) }
    }
    defer { accounts.retire() }
    accounts.setActive(false)
    let store = try #require(accounts.store(for: old.id))
    store.connection = .computerOffline(lastSeen: Date(milliseconds: 200))

    StaleCopyCloudStub.claimedAt.withLock { $0 = 100 }
    await accounts.refreshCloud()
    #expect(!accounts.isStale(old.id))

    StaleCopyCloudStub.claimedAt.withLock { $0 = 300 }
    await accounts.refreshCloud()
    #expect(accounts.isStale(old.id))
}
