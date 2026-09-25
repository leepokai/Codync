import Foundation
import CryptoKit

/// Values shared between the iOS app and its widgets through the App Group.
public enum SharedStore {
    public static let appGroup = "group.com.pokai.Codync"
    /// Codync push relay (relay/ in this repo).
    public static let relayURL = "https://codync-relay.kevin2005ha.workers.dev"

    private static var defaults: UserDefaults { UserDefaults(suiteName: appGroup) ?? .standard }

    /// Widgets read only the account selected by the app. Stores capture a fixed
    /// context so late responses from a previous account cannot write into it.
    public static var activeAccountID: String? {
        get { defaults.string(forKey: "activeAccountID") }
        set { defaults.set(newValue, forKey: "activeAccountID") }
    }

    public static var activeContext: Context { Context(accountID: activeAccountID) }

    public struct Context {
        public let accountID: String?
        private let defaults: UserDefaults
        public var id: String { accountID.map { Self.digest($0) } ?? "local" }

        public init(accountID: String?, defaults: UserDefaults = UserDefaults(suiteName: SharedStore.appGroup) ?? .standard) {
            self.accountID = accountID
            self.defaults = defaults
        }

        private func key(_ name: String) -> String { accountID == nil ? name : "account.\(id).\(name)" }
        public static func digest(_ value: String) -> String {
            SHA256.hash(data: Data(value.utf8)).map { String(format: "%02x", $0) }.joined()
        }
        private func read<T: Decodable>(_ type: T.Type, _ name: String) -> T? {
            defaults.data(forKey: key(name)).flatMap { try? JSONDecoder().decode(type, from: $0) }
        }
        private func write<T: Encodable>(_ value: T?, _ name: String) {
            defaults.set(value.flatMap { try? JSONEncoder().encode($0) }, forKey: key(name))
        }
        public var pairing: Pairing? {
            get { read(Pairing.self, "pairing") }
            nonmutating set { write(newValue, "pairing") }
        }
        public var computers: [Pairing] {
            get { read([Pairing].self, "computers") ?? pairing.map { [$0] } ?? [] }
            nonmutating set { write(newValue, "computers") }
        }
        public var preferredURL: String? {
            get { defaults.string(forKey: key("preferredURL")) }
            nonmutating set { defaults.set(newValue, forKey: key("preferredURL")) }
        }
        public var usage: Usage? {
            get { read(Usage.self, "usage") }
            nonmutating set { write(newValue, "usage") }
        }
        public var bots: [Bot] {
            get { read([Bot].self, "bots") ?? [] }
            nonmutating set { write(newValue, "bots") }
        }
        /// Deep links carry the storage scope captured when a widget/activity
        /// was created, so an old surface cannot open another account's bot.
        public func botURL(_ botID: String) -> URL {
            var url = URLComponents()
            url.scheme = "codync"
            url.host = "bot"
            url.path = "/" + botID
            url.queryItems = [
                URLQueryItem(name: "scope", value: id),
                URLQueryItem(name: "computer", value: Self.digest(pairing?.token ?? "unpaired"))
            ]
            return url.url!
        }
        public func acceptsBotURL(_ url: URL) -> Bool {
            guard url.scheme == "codync", url.host == "bot" else { return false }
            let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
            guard let scope = items.first(where: { $0.name == "scope" })?.value else {
                return accountID == nil
            }
            return scope == id && items.first(where: { $0.name == "computer" })?.value == Self.digest(pairing?.token ?? "unpaired")
        }
        public var orderedPairing: Pairing? {
            guard var p = pairing else { return nil }
            if let preferredURL, let i = p.urls.firstIndex(of: preferredURL) {
                p.urls.insert(p.urls.remove(at: i), at: 0)
            }
            return p
        }
    }

    public static var pairing: Pairing? {
        get { activeContext.pairing }
        set { activeContext.pairing = newValue }
    }
    public static var computers: [Pairing] {
        get { activeContext.computers }
        set { activeContext.computers = newValue }
    }
    public static var preferredURL: String? {
        get { activeContext.preferredURL }
        set { activeContext.preferredURL = newValue }
    }
    public static var usage: Usage? {
        get { activeContext.usage }
        set { activeContext.usage = newValue }
    }
    public static var bots: [Bot] {
        get { activeContext.bots }
        set { activeContext.bots = newValue }
    }
    public static var orderedPairing: Pairing? { activeContext.orderedPairing }
}
