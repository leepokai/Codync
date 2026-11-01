import Foundation
import os
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// The cloud's certificate route: 403 until this device registers, then certificates expiring at `exp`.
private final class CertificateCloudStub: URLProtocol {
    struct State {
        var registered = false
        var offline = false
        var exp: Int64 = 0
        var issued = 0
    }

    static let state = OSAllocatedUnfairLock(initialState: State())

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() {}

    override func startLoading() {
        let path = request.url?.path ?? ""
        let answer: (status: Int, json: String)? = Self.state.withLock { s in
            if s.offline { return nil }
            if path.hasSuffix("/v1/devices") {
                s.registered = true
                return (200, "{}")
            }
            guard s.registered else { return (403, #"{"error":{"code":"forbidden"}}"#) }
            s.issued += 1
            return (200, #"{"certificate":{"payload":"{\"v\":1,\"exp\":\#(s.exp)}","sig":"s"}}"#)
        }
        guard let answer else {
            client?.urlProtocol(self, didFailWithError: URLError(.notConnectedToInternet))
            return
        }
        let response = HTTPURLResponse(url: request.url!, statusCode: answer.status, httpVersion: nil, headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(answer.json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
}

/// Pairing's certificate: an unregistered device registers once and gets one; it's held while more than a
/// day is left, renewed after, and offline a still valid one will do. Offline with none, pairing can't start.
@MainActor @Test func pairingCertificateIsRegisteredForHeldAndRenewed() async throws {
    let suite = "PairingCertificate.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [CertificateCloudStub.self]
    let cloud = CloudClient(baseURL: URL(string: "https://cloud.test")!, identity: DeviceIdentity(signing: .init(), push: .init()),
                            session: URLSession(configuration: config)) { "token" }
    let storage = SharedStore.Context(accountID: "account", suite: suite)
    let accounts = AccountStore(storage: storage, cloud: cloud)
    defer { accounts.retire() }
    func certificate(_ store: AccountStore) async throws -> DeviceCertificate {
        try await store.pairingCertificate(cloud, deviceName: "iPhone", platform: "ios")
    }
    func set(_ change: @escaping @Sendable (inout CertificateCloudStub.State) -> Void) { CertificateCloudStub.state.withLock(change) }
    let hour: Int64 = 3_600_000
    let now = Int64(Date.now.timeIntervalSince1970 * 1000)

    set { $0.exp = now + 12 * hour }
    let short = try await certificate(accounts)
    #expect(short.exp == now + 12 * hour)
    #expect(CertificateCloudStub.state.withLock { $0.registered })

    // Less than a day left: offline, it still does; online, a new one replaces it.
    set { $0.offline = true }
    #expect(try await certificate(accounts) == short)
    set {
        $0.offline = false
        $0.exp = now + 7 * 24 * hour
    }
    let long = try await certificate(accounts)
    #expect(long.exp == now + 7 * 24 * hour)
    #expect(try await certificate(accounts) == long)
    #expect(CertificateCloudStub.state.withLock { $0.issued } == 2)

    set { $0.offline = true }
    let fresh = AccountStore(storage: storage, cloud: cloud)
    defer { fresh.retire() }
    await #expect(throws: AccountStore.noCertificate) { try await certificate(fresh) }
    #expect(AccountStore.noCertificate.localizedDescription == "Connect to the internet to pair, so Codync can confirm your account.")
}
