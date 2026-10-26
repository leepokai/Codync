import CryptoKit
import Foundation
import os
import Testing
@testable import CodyncKit
@testable import CodyncUI

private let renewalHostKey = Data(repeating: 17, count: 32)

private final class RenewalCloudStub: URLProtocol {
    struct State {
        var deviceKey = ""
        var approved = false
        var calls: [String] = []
        var stallFirstRequest = false
        var created = 0
    }
    static let state = OSAllocatedUnfairLock(initialState: State())
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() {}

    override func startLoading() {
        let path = request.url!.path
        let method = request.httpMethod!
        let snapshot = Self.state.withLock { state in
            state.calls.append("\(method) \(path)")
            if method == "POST", path.hasSuffix("/access-requests") { state.created += 1 }
            return state
        }
        let id = RelayCrypto.computerId(signKey: renewalHostKey)
        let json: String
        if path == "/v1/computers" {
            json = #"{"computers":[{"computerId":"\#(id)","name":"Desk","signKey":"\#(renewalHostKey.base64URL)","online":true,"access":"granted"}]}"#
        } else if path == "/v1/devices" {
            json = #"{"devices":[{"deviceId":"this-phone","deviceKey":"\#(snapshot.deviceKey)","name":"Phone"},{"deviceId":"other-phone","deviceKey":"other","name":"Other phone"}]}"#
        } else if path.hasSuffix("/grants") {
            json = #"{"grants":[{"grantId":"own-grant","deviceId":"this-phone"},{"grantId":"other-grant","deviceId":"other-phone"}]}"#
        } else if path.hasSuffix("/access-requests") {
            let rid = snapshot.stallFirstRequest && snapshot.created == 1 ? "initial" : "renewal"
            json = #"{"requestId":"\#(rid)","expiresAt":4102444800000}"#
        } else if path == "/v1/access-requests/initial" {
            json = #"{"requestId":"initial","status":"pending"}"#
        } else if path == "/v1/access-requests/renewal" {
            json = #"{"requestId":"renewal","status":"\#(snapshot.approved ? "approved" : "pending")","hostNonce":"\#(Data(repeating: 9, count: 32).base64URL)"}"#
        } else {
            json = "{}"
        }
        client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
}

@Suite(.serialized) @MainActor
struct AccessRenewalTests {
    @Test func renewingAccessKeepsTheComputerAndOnlyReplacesThisPhonesApproval() async throws {
        let suite = "AccessRenewal.\(UUID().uuidString)"
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [RenewalCloudStub.self]
        let identity = DeviceIdentity(signing: .init(), push: .init())
        RenewalCloudStub.state.withLock { $0 = .init(deviceKey: identity.publicKey) }
        let cloud = CloudClient(baseURL: URL(string: "https://cloud.test")!, identity: identity,
                                session: URLSession(configuration: config)) { "token" }
        let storage = SharedStore.Context(accountID: "account", suite: suite)
        let id = RelayCrypto.computerId(signKey: renewalHostKey)
        var computer = Computer(id: id, name: "Desk", signKey: renewalHostKey.base64URL, urls: ["http://192.168.1.20:19223"])
        computer.color = "blue"
        computer.route = .directOnly
        computer.notifications = false
        storage.computers = [computer]
        let accounts = AccountStore(storage: storage, cloud: cloud) { computer in
            BotStore(computer: computer, storage: storage) { _ in FakeRemote(.connecting) }
        }
        defer { accounts.retire() }
        accounts.setActive(false)
        await accounts.refreshCloud()
        #expect(accounts.approvedAccess.isEmpty)
        #expect(!RenewalCloudStub.state.withLock { $0.calls }.contains { $0.hasPrefix("DELETE") })

        try await accounts.requestAccess(id, renew: true)
        for _ in 0..<100 where accounts.pendingAccess[id] == nil {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(accounts.pendingAccess[id]?.requestId == "renewal")
        #expect(accounts.approvedAccess.isEmpty) // A saved computer is not this request's approval.
        #expect(storage.computers == [computer])
        let calls = RenewalCloudStub.state.withLock { $0.calls }
        #expect(calls.filter { $0.hasPrefix("DELETE") } == ["DELETE /v1/computers/\(id)/grants/own-grant"])
        let revoked = try #require(calls.firstIndex(of: "DELETE /v1/computers/\(id)/grants/own-grant"))
        let requested = try #require(calls.firstIndex(of: "POST /v1/computers/\(id)/access-requests"))
        #expect(revoked < requested)

        RenewalCloudStub.state.withLock { $0.approved = true }
        for _ in 0..<150 where accounts.approvedAccess[id] == nil {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(accounts.approvedAccess[id] == "renewal")
        #expect(accounts.pendingAccess[id] == nil)
        #expect(storage.computers.first?.color == "blue")
        #expect(storage.computers.first?.route == .directOnly)
        #expect(storage.computers.first?.notifications == false)
        #expect(storage.computers.first?.urls == computer.urls)
    }


    @Test func renewalCancelsANegotiatingRequestBeforeRevokingAndReplacingIt() async throws {
        let suite = "AccessRenewal.\(UUID().uuidString)"
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [RenewalCloudStub.self]
        let identity = DeviceIdentity(signing: .init(), push: .init())
        RenewalCloudStub.state.withLock { $0 = .init(deviceKey: identity.publicKey, stallFirstRequest: true) }
        let cloud = CloudClient(baseURL: URL(string: "https://cloud.test")!, identity: identity,
                                session: URLSession(configuration: config)) { "token" }
        let storage = SharedStore.Context(accountID: "account", suite: suite)
        let accounts = AccountStore(storage: storage, cloud: cloud)
        defer { accounts.retire() }
        await accounts.refreshCloud()
        let id = RelayCrypto.computerId(signKey: renewalHostKey)
        let first = Task { try await accounts.requestAccess(id) }
        for _ in 0..<100 where !RenewalCloudStub.state.withLock({ $0.calls.contains("GET /v1/access-requests/initial") }) {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(RenewalCloudStub.state.withLock { $0.created } == 1)
        // A normal second ask joins the negotiating request rather than creating another one.
        let joined = Task { try await accounts.requestAccess(id) }
        try await Task.sleep(for: .milliseconds(20))
        #expect(RenewalCloudStub.state.withLock { $0.created } == 1)
        try await accounts.requestAccess(id, renew: true)
        _ = await first.result
        _ = await joined.result
        let calls = RenewalCloudStub.state.withLock { $0.calls }
        let cancelled = try #require(calls.firstIndex(of: "DELETE /v1/access-requests/initial"))
        let revoked = try #require(calls.firstIndex(of: "DELETE /v1/computers/\(id)/grants/own-grant"))
        let replacement = try #require(calls.lastIndex(of: "POST /v1/computers/\(id)/access-requests"))
        #expect(cancelled < revoked && revoked < replacement)
        #expect(RenewalCloudStub.state.withLock { $0.created } == 2)
    }

}
