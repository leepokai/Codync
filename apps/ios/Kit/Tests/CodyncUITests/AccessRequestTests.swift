import CryptoKit
import Foundation
import os
import Testing
@testable import CodyncKit
@testable import CodyncUI

private let hostKey = Data(repeating: 7, count: 32)

/// One access request's cloud calls: the account lists the computer, the request is created,
/// the host's nonce is already there, the reveal is accepted; approval never comes.
private final class AccessCloudStub: URLProtocol {
    /// Access requests committed so far.
    static let commits = OSAllocatedUnfairLock(initialState: 0)

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() {}

    override func startLoading() {
        let path = request.url?.path ?? ""
        if path.hasSuffix("/access-requests") { Self.commits.withLock { $0 += 1 } }
        let json = if path.hasSuffix("/v1/computers") {
            #"{"computers":[{"computerId":"\#(RelayCrypto.computerId(signKey: hostKey))","name":"Desk","signKey":"\#(hostKey.base64URL)","online":true}]}"#
        } else if path.hasSuffix("/access-requests") {
            #"{"requestId":"r1","expiresAt":4102444800000}"#
        } else if path.hasSuffix("/v1/access-requests/r1") {
            #"{"requestId":"r1","status":"pending","hostNonce":"\#(Data(repeating: 9, count: 32).base64URL)"}"#
        } else {
            "{}"
        }
        let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
}

/// Signed in with an online computer this device can't use, nothing is asked until the user taps.
/// Closing the access sheet before the code arrives and opening it again makes one request, which
/// goes on in the store and ends with the code in the banner, not pending on the computer unrevealed.
@MainActor @Test func accessIsAskedOnlyOnTapAndOutlivesItsSheet() async throws {
    let suite = "AccessRequest.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [AccessCloudStub.self]
    let identity = DeviceIdentity(signing: .init(), push: .init())
    let cloud = CloudClient(baseURL: URL(string: "https://cloud.test")!, identity: identity,
                            session: URLSession(configuration: config)) { "token" }
    let storage = SharedStore.Context(accountID: "account", suite: suite)
    let accounts = AccountStore(storage: storage, cloud: cloud)
    defer { accounts.retire() }
    await accounts.refreshCloud()
    let id = RelayCrypto.computerId(signKey: hostKey)
    #expect(accounts.cloudComputers.map(\.computerId) == [id])
    #expect(accounts.pendingAccess.isEmpty)
    #expect(AccessCloudStub.commits.withLock { $0 } == 0)

    let sheet = Task { try await accounts.requestAccess(id) }
    sheet.cancel()
    let reopened = Task { try await accounts.requestAccess(id) }

    for _ in 0..<100 where accounts.pendingAccess[id] == nil {
        try await Task.sleep(for: .milliseconds(50))
    }
    #expect(accounts.pendingAccess[id]?.requestId == "r1")
    #expect(accounts.pendingAccess[id]?.code.count == 6)
    try await reopened.value
    #expect(AccessCloudStub.commits.withLock { $0 } == 1)
}
