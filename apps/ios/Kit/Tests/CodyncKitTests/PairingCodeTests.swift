import Foundation
import os
import Testing
@testable import CodyncKit

/// The cloud's pairing-code route: `QRTX-4821` resolves to its link, any other code is unknown.
private final class PairingCodeCloudStub: URLProtocol {
    static let requests = OSAllocatedUnfairLock(initialState: [URLRequest]())

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() {}

    override func startLoading() {
        let request = request
        Self.requests.withLock { $0.append(request) }
        let known = request.url?.path == "/v1/pairing-codes/QRTX-4821"
        let json = known ? #"{"link":"codync://pair?v=3"}"#
            : #"{"error":{"code":"notFound","message":"That code isn't valid. Check it on the computer."}}"#
        let response = HTTPURLResponse(url: request.url!, statusCode: known ? 200 : 404, httpVersion: nil, headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
}

/// 4 letters, an optional dash or space, 4 digits, in any case; everything else (links included) isn't a code.
@Test func shortPairingCodesAreRecognizedAndNormalized() {
    for typed in ["QRTX-4821", "qrtx4821", " Qrtx 4821\n", "QRTX4821"] {
        #expect(Pairing.shortCode(typed) == "QRTX-4821")
    }
    for other in ["QRT-4821", "QRTX-482", "QRTX--4821", "QRTX_4821", "1234-QRTX", "QRTXX4821", "codync://pair?v=3", ""] {
        #expect(Pairing.shortCode(other) == nil)
    }
}

/// The code is looked up with the account's session, unsigned; the cloud's message is what an unknown one shows.
@Test func aPairingCodeResolvesToItsLinkThroughTheCloud() async throws {
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [PairingCodeCloudStub.self]
    let cloud = CloudClient(baseURL: URL(string: "https://cloud.test")!, identity: DeviceIdentity(signing: .init(), push: .init()),
                            session: URLSession(configuration: config)) { "token" }

    #expect(try await cloud.pairingLink(code: "QRTX-4821") == "codync://pair?v=3")
    let request = try #require(PairingCodeCloudStub.requests.withLock { $0.last })
    #expect(request.httpMethod == "GET")
    #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer token")
    #expect(request.value(forHTTPHeaderField: "Codync-Sig") == nil)

    await #expect(throws: CloudError(status: 404, code: "notFound", message: "That code isn't valid. Check it on the computer.")) {
        try await cloud.pairingLink(code: "ABCD-1234")
    }
    #expect(CloudError(status: 404, code: "notFound", message: "That code isn't valid. Check it on the computer.").localizedDescription
            == "That code isn't valid. Check it on the computer.")
}

/// The paste field's placeholder and the cloud's two code messages read in every language the app speaks.
@Test func pairingCodeCopyIsTranslatedEverywhere() {
    for language in [AppLanguage.zh, .zhHant, .ja, .ko, .fr, .de, .es] {
        for phrase in ["Pairing link or code", "That code isn't valid. Check it on the computer.",
                       "That code expired or was used. Show a new one on the computer."] {
            #expect(L10n.tr(phrase, preference: language, systemIsChinese: false) != phrase, "\(language) \(phrase)")
        }
    }
}
