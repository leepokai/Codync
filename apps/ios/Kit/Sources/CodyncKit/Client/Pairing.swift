import Foundation

/// A scanned QR / pairing link (§4.1). Short-lived: only the resulting `Computer` is kept.
/// `codync://pair?v=3&name=…&id=<computerId>&sk=…&bk=…&code=…&urls=a,b&cloud=…`
public struct Pairing: Hashable, Sendable {
    public var computer: Computer
    public var code: String

    public enum Problem: LocalizedError, Sendable, Equatable {
        case notPairingLink
        /// The computer runs a Codync that speaks another protocol version.
        case outdatedHost
        case invalid

        public var errorDescription: String? {
            switch self {
            case .notPairingLink: "That isn't a Codync pairing code."
            case .outdatedHost: "Update Codync on your computer, then show a new pairing code."
            case .invalid: "This pairing code is damaged. Show a new one on your computer."
            }
        }
    }

    public init?(url: URL) {
        guard let p = try? Self.parse(url) else { return nil }
        self = p
    }

    public init?(string: String) {
        guard let p = try? Self.parse(string) else { return nil }
        self = p
    }

    /// A short code `codync-host pair` prints (`QRTX-4821`: 4 letters, an optional dash or space, 4 digits) as the
    /// cloud takes it, or nil for anything else. The cloud resolves it to the pairing link.
    public static func shortCode(_ text: String) -> String? {
        guard let match = text.trimmingCharacters(in: .whitespacesAndNewlines).wholeMatch(of: /([A-Za-z]{4})[- ]?([0-9]{4})/) else { return nil }
        return "\(match.1.uppercased())-\(match.2)"
    }

    public static func parse(_ string: String) throws(Problem) -> Pairing {
        guard let url = URL(string: string.trimmingCharacters(in: .whitespacesAndNewlines)) else { throw .notPairingLink }
        return try parse(url)
    }

    public static func parse(_ url: URL) throws(Problem) -> Pairing {
        guard url.scheme == AppEnvironment.current.urlScheme, url.host() == "pair",
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems else { throw .notPairingLink }
        let q = Dictionary(items.map { ($0.name, $0.value ?? "") }, uniquingKeysWith: { a, _ in a })
        guard q["v"] == "3" else { throw .outdatedHost }
        guard let id = q["id"], let sk = q["sk"], let bk = q["bk"], let code = q["code"],
              let skRaw = Data(base64URL: sk), skRaw.count == 32,
              Data(base64URL: bk)?.count == 32,
              Data(base64URL: code)?.count == 16,
              RelayCrypto.computerId(signKey: skRaw) == id else { throw .invalid }
        let urls = directURLs((q["urls"] ?? "").split(separator: ",").map(String.init))
        var cloud: URL?
        if let raw = q["cloud"], !raw.isEmpty {
            guard let u = URL(string: raw), Self.isCloudURL(u) else { throw .invalid }
            cloud = u
        }
        guard !urls.isEmpty || cloud != nil else { throw .invalid }
        let name = q["name"].flatMap { $0.isEmpty ? nil : $0 } ?? "My computer"
        return Pairing(computer: Computer(id: id, name: name, signKey: sk, boxKey: bk, urls: urls, cloud: cloud), code: code)
    }

    init(computer: Computer, code: String) {
        self.computer = computer
        self.code = code
    }

    /// The 6 digits the computer shows if it asks whether to let `identity` in.
    public func checkCode(for identity: DeviceIdentity) -> String {
        RelayCrypto.pairCheck(deviceKey: identity.deviceKey, hostSignKey: Data(base64URL: computer.signKey) ?? Data())
    }

    /// The relay's pairing socket parameter.
    var offerId: String? { Data(base64URL: code).map(RelayCrypto.offerId(code:)) }

    /// Phones keep at most this many direct addresses; a computer with many networks sends more, best first.
    static let maxURLs = 16

    /// The direct addresses kept from a computer's list (pairing link or `hello`): valid ones, once each, the best `maxURLs`.
    static func directURLs(_ urls: [String]) -> [String] {
        var seen = Set<String>()
        return Array(urls.filter { isDirectURL($0) && seen.insert($0).inserted }.prefix(maxURLs))
    }

    static func isDirectURL(_ s: String) -> Bool {
        guard let u = URL(string: s), u.scheme == "http" || u.scheme == "https", u.host() != nil else { return false }
        return true
    }

    /// https only; debug builds also allow a local dev cloud.
    public static func isCloudURL(_ url: URL) -> Bool {
        guard url.host() != nil else { return false }
        if url.scheme == "https" { return true }
        #if DEBUG
        return url.scheme == "http" && ["127.0.0.1", "localhost"].contains(url.host())
        #else
        return false
        #endif
    }
}
