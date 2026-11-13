import CodyncKit
import Foundation

extension BotStore {
    private struct Cache: Codable {
        /// What the copy was synced under (`syncStamp`); another stamp means refetch everything.
        var stamp: String?
        var hostId: String?
        var rev: Int64
        var bots: [Bot]
        var entries: [Entry]
        var icons: [String: CustomIcon]?
    }

    /// Raise when the app starts to need a field the rows it cached may lack.
    static let mirrorFormat = 1

    /// The mirror format and the host's store schema (its version on a host that sends none): updates of the app
    /// or the host that keep both keep the copy and its cursor.
    static func syncStamp(_ hello: Hello) -> String {
        "\(mirrorFormat):\(hello.schema.map { "schema \($0)" } ?? hello.version)"
    }

    /// Per app, context and computer (`SharedStore.Context.erase()` removes a context's files).
    private var cacheURL: URL {
        URL.cachesDirectory.appending(path: "codync-mirror-\(Bundle.main.bundleIdentifier ?? "app")-\(storage.id)-\(computer.id).json")
    }

    func loadCache() {
        // A cache synced under another stamp still shows at once; the next hello fetches everything
        // again underneath it.
        guard let data = try? Data(contentsOf: cacheURL),
              let cache = try? JSONDecoder().decode(Cache.self, from: data) else { return }
        cacheStamp = cache.stamp ?? ""
        hostId = cache.hostId
        rev = cache.rev
        bots = Dictionary(cache.bots.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        entries = Dictionary(grouping: cache.entries.map(Self.afterRelaunch), by: \.botId)
        applyIcons(cache.icons ?? [:])
    }

    /// A message on its way when the app went may or may not have arrived: failed, with a Resend that reuses its nonce.
    /// If the computer got it, the events catch-up replaces it by clientNonce.
    private static func afterRelaunch(_ entry: Entry) -> Entry {
        guard entry.id.hasPrefix("local-"), entry.data.status == "sending" else { return entry }
        var entry = entry
        entry.data.status = "failed"
        return entry
    }

    func scheduleSave() {
        guard !retired else { return }
        saveTask?.cancel()
        saveTask = Task {
            try? await Task.sleep(for: .seconds(2))
            if !Task.isCancelled { saveCache() }
        }
    }

    public func saveCache() {
        guard !retired else { return }
        // Keep the newest 200 entries per bot; older ones page in from the host. Local messages the
        // computer may not have stay visible so they can be cancelled or retried after a relaunch.
        let kept = entries.values.flatMap { list in
            list.filter { !$0.id.hasPrefix("local-") }.suffix(200)
                + list.filter { $0.id.hasPrefix("local-") && ["sending", "waiting", "held", "delivering", "failed"].contains($0.data.status) }
        }
        let cachedBots = bots.values.map { bot in var bot = bot; bot.customIcon = nil; return bot }
        // Saved partway through a refetch, the cache says so: the next launch begins it again.
        let stamp = refetched == nil ? cacheStamp ?? "" : ""
        let cache = Cache(stamp: stamp, hostId: hostId, rev: rev, bots: cachedBots, entries: kept, icons: icons)
        if let data = try? JSONEncoder().encode(cache) {
            try? data.write(to: cacheURL, options: .atomic)
        }
    }
}
