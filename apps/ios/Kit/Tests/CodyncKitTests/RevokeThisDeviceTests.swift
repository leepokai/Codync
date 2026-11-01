import Foundation
import os
import Testing
@testable import CodyncKit

/// Answers `GET /v1/devices` with this phone and another one, and records every call with its token.
private final class DevicesStub: URLProtocol {
    static let state = OSAllocatedUnfairLock(initialState: (deviceKey: "", calls: [String]()))
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() {}

    override func startLoading() {
        let call = "\(request.httpMethod!) \(request.url!.path) \(request.value(forHTTPHeaderField: "Authorization") ?? "")"
        let key = Self.state.withLock { state in
            state.calls.append(call)
            return state.deviceKey
        }
        let json = request.url!.path == "/v1/devices"
            ? #"{"devices":[{"deviceId":"other-phone","deviceKey":"other","name":"Other"},{"deviceId":"this-phone","deviceKey":"\#(key)","name":"Phone"}]}"#
            : "{}"
        client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
}

@Suite(.serialized) struct RevokeThisDeviceTests {
    @Test func signingOutDeletesOnlyThisDeviceOnTheTokenTakenBefore() async throws {
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [DevicesStub.self]
        let identity = DeviceIdentity(signing: .init(), push: .init())
        DevicesStub.state.withLock { $0 = (identity.publicKey, []) }
        let signedIn = OSAllocatedUnfairLock(initialState: true)
        let cloud = CloudClient(baseURL: URL(string: "https://cloud.test")!, identity: identity, session: URLSession(configuration: config)) {
            guard signedIn.withLock({ $0 }) else { throw CloudError(status: 401, code: "signedOut") }
            return "token"
        }

        let pinned = try #require(await cloud.pinningToken())
        signedIn.withLock { $0 = false }
        try await pinned.revokeThisDevice()

        #expect(DevicesStub.state.withLock { $0.calls } == ["GET /v1/devices Bearer token", "DELETE /v1/devices/this-phone Bearer token"])
        #expect(await cloud.pinningToken() == nil, "signed out: no token to take")
    }
}
