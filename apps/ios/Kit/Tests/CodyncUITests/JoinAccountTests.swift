import Foundation
import os
import Testing
@testable import CodyncKit
@testable import CodyncUI

private let askingHello = #"{"hostId":"h1","name":"Mac","version":"3.0.0","os":"macos","backends":[],"rev":0,"joinAccount":true}"#

/// A computer whose `hello` says this phone paired without it joining the account asks the account to finish the
/// join; one that doesn't say so (or an older host) doesn't.
@MainActor @Test(arguments: [false, true]) func helloAsksToFinishJoiningTheAccount(asks: Bool) async throws {
    let suite = "JoinAccountTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let remote = FakeRemote(.ready(.direct))
    if asks { await remote.setHello(askingHello) }
    let computer = Computer(id: UUID().uuidString, name: "Mac", signKey: "unused", boxKey: "unused")
    let store = BotStore(computer: computer, storage: SharedStore.Context(accountID: "account", suite: suite)) { _ in remote }
    defer { store.retire() }
    var asked = 0
    store.onJoinAccount = { _ in asked += 1 }
    store.setActive(true)
    #expect(await until { store.hello != nil })
    #expect(asked == (asks ? 1 : 0))
}

/// Answers the claim calls (counting them) and every other request as an empty account.
private final class ClaimCloudStub: URLProtocol {
    static let created = OSAllocatedUnfairLock(initialState: 0)
    static let completed = OSAllocatedUnfairLock(initialState: 0)

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() {}

    override func startLoading() {
        let path = request.url?.path ?? ""
        let json: String
        switch path {
        case "/v1/claims":
            Self.created.withLock { $0 += 1 }
            json = #"{"claimId":"clm_1","nonce":"n1"}"#
        case "/v1/claims/clm_1/complete":
            Self.completed.withLock { $0 += 1 }
            json = "{}"
        case "/v1/computers": json = #"{"computers":[]}"#
        default: json = "{}"
        }
        let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
}

/// The account finishes the join its `hello` asks for: a claim, the computer's signature, then the completion.
@MainActor @Test func theAccountFinishesTheJoinTheComputerAsksFor() async throws {
    let suite = "JoinAccountTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [ClaimCloudStub.self]
    let cloud = CloudClient(baseURL: URL(string: "https://cloud.test")!, identity: DeviceIdentity(signing: .init(), push: .init()),
                            session: URLSession(configuration: config)) { "token" }
    let storage = SharedStore.Context(accountID: "account", suite: suite)
    storage.computers = [randomComputer("Mac")]
    let remote = FakeRemote(.ready(.direct))
    await remote.setHello(askingHello)
    let accounts = AccountStore(storage: storage, cloud: cloud) { computer in
        BotStore(computer: computer, storage: storage) { _ in remote }
    }
    defer { accounts.retire() }

    #expect(await until { ClaimCloudStub.completed.withLock { $0 } == 1 })
    #expect(ClaimCloudStub.created.withLock { $0 } == 1)
    #expect(await remote.calls.filter { $0 == "joinAccount" }.count == 1)
}
