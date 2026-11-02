import CryptoKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Account")

/// Values shared between the app, its widgets and its notification extension through the App Group,
/// one namespace per account context so a signed-out account's data never shows in another.
public enum SharedStore {
    public static let appGroup = AppEnvironment.current.appGroup
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
        fileprivate let suite: String
        fileprivate var defaults: UserDefaults { UserDefaults(suiteName: suite) ?? .standard }
        public var id: String { accountID.map { Self.digest($0) } ?? "local" }

        public init(accountID: String?, suite: String = SharedStore.appGroup) {
            self.accountID = accountID
            self.suite = suite
        }

        fileprivate static let names = ["computers", "bots", "usage", "lastComputerId", "composerDrafts", "computerOrder", "shortcutLeadComputer"]
        fileprivate func key(_ name: String) -> String { accountID == nil ? name : "account.\(id).\(name)" }
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
        /// Device-local ordering, isolated by account.
        public var computerOrder: ComputerOrder? {
            get { read(ComputerOrder.self, "computerOrder") }
            nonmutating set { write(newValue, "computerOrder") }
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
        /// Unsent text, isolated by account, computer and bot; not a transcript cache.
        public var composerDrafts: [ComputerID: [String: String]] {
            get { read([ComputerID: [String: String]].self, "composerDrafts") ?? [:] }
            nonmutating set { write(newValue, "composerDrafts") }
        }

        /// The computer whose lead a shortcut that names no bot calls (Settings); nil: the first computer with one.
        public var shortcutLeadComputer: ComputerID? {
            get { defaults.string(forKey: key("shortcutLeadComputer")) }
            nonmutating set { defaults.set(newValue, forKey: key("shortcutLeadComputer")) }
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
            url.scheme = AppEnvironment.current.urlScheme
            url.host = "bot"
            url.path = "/" + ref.botId
            url.queryItems = [
                URLQueryItem(name: "scope", value: id),
                URLQueryItem(name: "computer", value: ref.computerId),
            ]
            return url.url ?? AppEnvironment.current.link("bot")
        }

        /// The bot a deep link points at, or nil when it belongs to another context or is malformed.
        public func reference(from url: URL) -> BotReference? {
            guard url.scheme == AppEnvironment.current.urlScheme, url.host() == "bot",
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
            do { try ActivityIconCache.erase(context: id) }
            catch { Logger(subsystem: "com.pokai.Codync", category: "CustomIcon").error("Cannot erase activity icons: \(error.localizedDescription)") }
            for file in mirrorCaches() { try? FileManager.default.removeItem(at: file) }
        }

        /// This context's mirror caches (`BotStore.saveCache`): `codync-mirror-<app>-<id>-<computer>.json`.
        fileprivate func mirrorCaches() -> [URL] {
            let caches = URL.cachesDirectory
            return ((try? FileManager.default.contentsOfDirectory(atPath: caches.path())) ?? [])
                .filter { $0.hasPrefix("codync-mirror-") && $0.contains("-\(id)-") }
                .map { caches.appending(path: $0) }
        }
    }
}

// MARK: signed-out pairings move into the account

public extension SharedStore.Context {
    /// What `adopt(from:)` did with the signed-out context's pairings: how many computers came over, and whether
    /// this iPhone's keys came with them (`moved`: this context was new, so they keep working) or they have to be
    /// paired again (`rescan`: this context has its own key).
    struct Adoption: Equatable, Sendable {
        public let computers: Int
        public let keys: AnalyticsEvent.MovedKeys
    }

    /// Sign-in is required: the signed-out context's computers move into this account context, which
    /// then erases it. Runs before this context's stores and keys load. Nil when there was nothing to move.
    func adopt(from local: Self) -> Adoption? {
        guard accountID != nil, local.accountID == nil, local.suite == suite else { return nil }
        let moving = local.computers
        guard !moving.isEmpty || DeviceIdentity.exists(context: local) else { return nil }
        defer { local.erase() }
        if !DeviceIdentity.exists(context: self), computers.isEmpty {
            do {
                try DeviceIdentity.move(from: local, to: self)
            } catch {
                log.error("Cannot move the device key: \(error.localizedDescription)")
                return merge(moving)
            }
            for name in Self.names { defaults.set(local.defaults.object(forKey: local.key(name)), forKey: key(name)) }
            moveMirrorCaches(from: local)
            return Adoption(computers: moving.count, keys: .moved)
        }
        return merge(moving)
    }

    /// Unsent and failed messages live only in the mirror caches: they come along rather than go with `erase()`.
    /// A rename in one folder: only a cache left at the account's name (it has no computers yet) is in the way.
    private func moveMirrorCaches(from local: Self) {
        for file in local.mirrorCaches() {
            let name = file.lastPathComponent
            guard let marker = name.range(of: "-\(local.id)-") else { continue }
            let moved = file.deletingLastPathComponent().appending(path: name.replacingCharacters(in: marker, with: "-\(id)-"))
            do {
                if FileManager.default.fileExists(atPath: moved.path()) { try FileManager.default.removeItem(at: moved) }
                try FileManager.default.moveItem(at: file, to: moved)
            } catch {
                log.error("Cannot move unsent messages: \(error.localizedDescription)")
            }
        }
    }

    private func merge(_ moving: [Computer]) -> Adoption {
        let known = Set(computers.map(\.id))
        computers += moving.filter { !known.contains($0.id) }
        return Adoption(computers: moving.count, keys: .rescan)
    }
}

public extension SharedStore.Context {
    /// Every usage provider this account's computers have reported, plus Claude and Codex so
    /// there's a choice before the first report. Sorted by name.
    var usageProviders: [(id: String, name: String)] {
        var names = ["claude": "Claude", "codex": "Codex"]
        for report in usage.values { for provider in report.providers { names[provider.id] = provider.name } }
        return names.map { (id: $0.key, name: $0.value) }.sorted { $0.name < $1.name }
    }

    /// 2.28.0 kept one push switch for every computer (`notificationsEnabled` in `defaults`). Switched off, it
    /// becomes the switch of every computer saved in `contexts`; the old key goes either way, so this runs once.
    static func splitPushSwitch(from defaults: UserDefaults, into contexts: [Self]) {
        let key = "notificationsEnabled"
        defer { defaults.removeObject(forKey: key) }
        guard defaults.object(forKey: key) as? Bool == false else { return }
        for context in contexts {
            context.computers = context.computers.map { var computer = $0; computer.notifications = false; return computer }
        }
    }
}
