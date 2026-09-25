import CryptoKit
import Foundation

/// Values shared between the app, its widgets and its notification extension through the App Group,
/// one namespace per account context so a signed-out account's data never shows in another.
public enum SharedStore {
    public static let appGroup = "group.com.pokai.Codync"
    /// The APNs push relay (relay/ in this repo).
    public static let relayURL = "https://codync-relay.kevin2005ha.workers.dev"

    private static var defaults: UserDefaults { UserDefaults(suiteName: appGroup) ?? .standard }

    /// Widgets read only the account selected by the app. Stores capture a fixed
    /// context so late responses from a previous account cannot write into it.
    public static var activeAccountID: String? {
        get { defaults.string(forKey: "activeAccountID") }
        set { defaults.set(newValue, forKey: "activeAccountID") }
    }

    public static var activeContext: Context { Context(accountID: activeAccountID) }

    public struct Context: Sendable {
        /// The raw Clerk user ID; nil = the local (signed-out) context.
        public let accountID: String?
        /// The UserDefaults suite (the App Group; tests use their own).
        private let suite: String
        private var defaults: UserDefaults { UserDefaults(suiteName: suite) ?? .standard }
        public var id: String { accountID.map { Self.digest($0) } ?? "local" }

        public init(accountID: String?, suite: String = SharedStore.appGroup) {
            self.accountID = accountID
            self.suite = suite
        }

        private static let names = ["computers", "bots", "usage", "lastComputerId"]
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

        /// Computers this context may reach (no secrets: pinned public keys and addresses).
        public var computers: [Computer] {
            get { read([Computer].self, "computers") ?? [] }
            nonmutating set { write(newValue, "computers") }
        }
        /// The widgets' snapshot of every computer's bots.
        public var bots: [BotSnapshot] {
            get { read([BotSnapshot].self, "bots") ?? [] }
            nonmutating set { write(newValue, "bots") }
        }
        public var usage: [ComputerID: Usage] {
            get { read([ComputerID: Usage].self, "usage") ?? [:] }
            nonmutating set { write(newValue, "usage") }
        }
        /// The most recently active computer; the Usage widget shows only this one.
        public var lastComputerId: ComputerID? {
            get { defaults.string(forKey: key("lastComputerId")) }
            nonmutating set { defaults.set(newValue, forKey: key("lastComputerId")) }
        }

        /// Deep links carry the context they were made in, so an old widget or activity
        /// can't open another account's bot: `codync://bot/<botId>?scope=<id>&computer=<computerId>`.
        public func botURL(_ ref: BotReference) -> URL {
            var url = URLComponents()
            url.scheme = "codync"
            url.host = "bot"
            url.path = "/" + ref.botId
            url.queryItems = [
                URLQueryItem(name: "scope", value: id),
                URLQueryItem(name: "computer", value: ref.computerId),
            ]
            return url.url ?? URL(string: "codync://bot")!
        }

        /// The bot a deep link points at, or nil when it belongs to another context or is malformed.
        public func reference(from url: URL) -> BotReference? {
            guard url.scheme == "codync", url.host() == "bot",
                  let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
                  items.first(where: { $0.name == "scope" })?.value == id,
                  let computer = items.first(where: { $0.name == "computer" })?.value,
                  RelayCrypto.computerIdRaw(computer) != nil else { return nil }
            let botId = String(url.path(percentEncoded: false).dropFirst())
            guard !botId.isEmpty, !botId.contains("/") else { return nil }
            return BotReference(accountId: accountID, computerId: computer, botId: botId)
        }

        /// Signing out or removing the account: device keys, computers, widget snapshot and caches go.
        public func erase() {
            for name in Self.names { defaults.removeObject(forKey: key(name)) }
            DeviceIdentity.delete(context: self)
            let caches = URL.cachesDirectory
            let marker = "-\(id)-"
            for file in (try? FileManager.default.contentsOfDirectory(atPath: caches.path())) ?? []
            where file.hasPrefix("codync-mirror-") && file.contains(marker) {
                try? FileManager.default.removeItem(at: caches.appending(path: file))
            }
        }
    }
}
