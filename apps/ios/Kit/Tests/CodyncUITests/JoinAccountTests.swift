import Foundation
import os
import Testing
@testable import CodyncKit
@testable import CodyncUI

private let joinableHello = #"{"hostId":"h1","name":"Mac","version":"3.0.0","os":"macos","backends":[],"rev":0,"canJoinAccount":true}"#

/// Answers the claim calls (counting them) and every other request as an empty account.
private final class ClaimCloudStub: URLProtocol {
    static let created = OSAllocatedUnfairLock(initialState: 0)
    static let completed = OSAllocatedUnfairLock(initialState: 0)
    /// The computer the account lists.
    static let listed = OSAllocatedUnfairLock<Computer?>(initialState: nil)

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
        case "/v1/computers":
            json = Self.listed.withLock { $0 }.map {
                #"{"computers":[{"computerId":"\#($0.id)","name":"Mac","signKey":"\#($0.signKey)","online":true}]}"#
            } ?? #"{"computers":[]}"#
        case "/v1/devices/certificate": json = #"{"certificate":{"payload":"{\"exp\":99999999999999}","sig":"s"}}"#
        default: json = "{}"
        }
        let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
}

@MainActor private func accounts(suite: String, computerCloud: URL? = URL(string: "https://cloud.example.dev"), listed: Bool = false,
                                  link: LinkState = .ready(.direct)) async -> (AccountStore, FakeRemote) {
    Keychain.memory.withLock { $0 = $0 ?? [:] }
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [ClaimCloudStub.self]
    let cloud = CloudClient(baseURL: URL(string: "https://cloud.test")!, identity: DeviceIdentity(signing: .init(), push: .init()),
                            session: URLSession(configuration: config)) { "token" }
    let storage = SharedStore.Context(accountID: "account", suite: suite)
    var paired = randomComputer("Mac")
    paired.cloud = computerCloud
    storage.computers = [paired]
    let computer = paired
    ClaimCloudStub.listed.withLock { $0 = listed ? computer : nil }
    let remote = FakeRemote(link)
    await remote.setHello(joinableHello)
    let accounts = AccountStore(storage: storage, cloud: cloud) { computer in
        BotStore(computer: computer, storage: storage) { _ in remote }
    }
    return (accounts, remote)
}

@MainActor private func online(_ remote: FakeRemote) async {
    #expect(await until { await remote.subscribed })
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
}

@Suite(.serialized) @MainActor struct JoinAccountTests {
    /// A computer in no account stays out of one until someone asks: the phone only offers.
    @Test func aComputerThatCouldJoinIsOnlyOffered() async throws {
        let suite = "JoinAccountTests.\(UUID().uuidString)"
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        let created = ClaimCloudStub.created.withLock { $0 }
        let (accounts, remote) = await accounts(suite: suite)
        defer { accounts.retire() }
        let store = try #require(accounts.computers.first.flatMap { accounts.store(for: $0.id) })
        await online(remote)
        #expect(await until { accounts.canAddToAccount(store) })
        #expect(ClaimCloudStub.created.withLock { $0 } == created)
        #expect(await remote.calls.filter { $0 == "joinAccount" }.isEmpty)
    }

    /// Asked to: a claim, the computer's signature (it asks its user), then the completion.
    @Test func addToAccountAsksTheComputerThenCompletesTheClaim() async throws {
        let suite = "JoinAccountTests.\(UUID().uuidString)"
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        let completed = ClaimCloudStub.completed.withLock { $0 }
        let (accounts, remote) = await accounts(suite: suite)
        defer { accounts.retire() }
        let store = try #require(accounts.computers.first.flatMap { accounts.store(for: $0.id) })
        await online(remote)
        #expect(await until { accounts.canAddToAccount(store) })
        accounts.addToAccount(store.computer.id, deviceName: "iPhone", platform: "ios")
        #expect(await until { ClaimCloudStub.completed.withLock { $0 } == completed + 1 })
        #expect(await remote.calls.filter { $0 == "joinAccount" }.count == 1)
        #expect(await until { accounts.addToAccountCheck(store.computer.id) == nil }, "the code goes once it's answered")
    }

    /// It joined while this phone was away: the account's list brings the relay its `hello` couldn't.
    @Test func aComputerListedInTheAccountGetsTheRelay() async throws {
        let suite = "JoinAccountTests.\(UUID().uuidString)"
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        let (accounts, _) = await accounts(suite: suite, computerCloud: nil, listed: true, link: .failed("Can't reach it."))
        defer { accounts.retire() }
        let store = try #require(accounts.computers.first.flatMap { accounts.store(for: $0.id) })
        #expect(store.computer.cloud == nil)
        await accounts.refreshCloud()
        #expect(store.computer.cloud?.host() == "cloud.test")
        #expect(accounts.storage.computers.first?.cloud?.host() == "cloud.test", "and it stays")
    }

    /// Signed out, the Codync relay isn't a way in: with no other address there's nothing to dial.
    @Test func signedOutAComputerReachableOnlyThroughTheCodyncRelayCantPair() async throws {
        let suite = "JoinAccountTests.\(UUID().uuidString)"
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        Keychain.memory.withLock { $0 = $0 ?? [:] }
        let storage = SharedStore.Context(accountID: nil, suite: suite)
        defer { storage.erase() }
        let accounts = AccountStore(storage: storage, cloud: nil) { computer in
            BotStore(computer: computer, storage: storage) { _ in FakeRemote(.connecting) }
        }
        defer { accounts.retire() }
        var computer = randomComputer("Mac")
        computer.urls = []
        computer.cloud = URL(string: "https://api.codync.dev")
        do {
            _ = try await accounts.pair(Pairing(computer: computer, code: "CAgICAgICAgICAgICAgICA"), deviceName: "iPhone", platform: "ios")
            Issue.record("it paired")
        } catch let error as CloudError {
            #expect(error.code == AccountStore.noDirectAddress.code)
        }
    }

    /// With no account there's nothing to add it to.
    @Test func signedOutThereIsNothingToOffer() async throws {
        let suite = "JoinAccountTests.\(UUID().uuidString)"
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        let storage = SharedStore.Context(accountID: nil, suite: suite)
        storage.computers = [randomComputer("Mac")]
        let remote = FakeRemote(.ready(.direct))
        await remote.setHello(joinableHello)
        let accounts = AccountStore(storage: storage, cloud: nil) { computer in
            BotStore(computer: computer, storage: storage) { _ in remote }
        }
        defer { accounts.retire() }
        let store = try #require(accounts.computers.first.flatMap { accounts.store(for: $0.id) })
        #expect(await until { store.hello != nil })
        #expect(!accounts.canAddToAccount(store))
    }
}
