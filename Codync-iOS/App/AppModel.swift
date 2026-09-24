import CodyncKit
import Foundation
import Observation
import UIKit
import WidgetKit
import os

private let log = Logger(subsystem: "com.pokai.Codync.ios", category: "AppModel")

/// Single source of truth for the phone: mirrors the host's bots and
/// transcripts via one SSE stream (catch-up since `rev`, then live).
@MainActor
@Observable
final class AppModel {
    enum Connection: Equatable {
        case unpaired
        case connecting
        case online
        case offline(String)
    }

    private(set) var pairing: Pairing? = SharedStore.pairing
    private(set) var connection: Connection = .unpaired
    private(set) var client: HostClient?
    private(set) var hello: Hello?
    private(set) var bots: [String: Bot] = [:]
    private(set) var entries: [String: [Entry]] = [:]
    private(set) var usage = SharedStore.usage ?? Usage()
    /// Bots whose older history has been fully paged in.
    private(set) var historyComplete: Set<String> = []
    var lastError: String?

    private var rev: Int64 = 0
    private var hostId: String?
    private var streamTask: Task<Void, Never>?
    private var isActive = true
    private var saveTask: Task<Void, Never>?
    private var rewound = false

    init() {
        loadCache()
        connection = pairing == nil ? .unpaired : .connecting
    }

    // MARK: derived

    var hostName: String { hello?.name ?? pairing?.name ?? "Computer" }

    /// Roster order: pinned first (manual order not tracked yet), then most recent activity.
    var roster: [Bot] {
        bots.values.filter { !$0.hidden }.sorted {
            if $0.pinned != $1.pinned { return $0.pinned }
            return $0.lastAt > $1.lastAt
        }
    }

    var hiddenBots: [Bot] { bots.values.filter(\.hidden).sorted { $0.name < $1.name } }

    func thread(_ botId: String) -> [Entry] { entries[botId] ?? [] }

    // MARK: pairing

    func pair(_ p: Pairing) {
        let changedHost = pairing?.token != p.token
        pairing = p
        SharedStore.pairing = p
        SharedStore.preferredURL = nil
        if changedHost { resetMirror() }
        connection = .connecting
        restartStream()
        PushRegistrar.shared.syncDevice(with: self)
    }

    func unpair() {
        streamTask?.cancel()
        pairing = nil
        client = nil
        hello = nil
        SharedStore.pairing = nil
        resetMirror()
        connection = .unpaired
    }

    private func resetMirror() {
        bots = [:]
        entries = [:]
        rev = 0
        hostId = nil
        historyComplete = []
        saveCache()
    }

    // MARK: lifecycle

    func setActive(_ active: Bool) {
        isActive = active
        if active {
            restartStream()
        } else {
            // Disconnect so the host knows we're gone and sends pushes instead.
            streamTask?.cancel()
            streamTask = nil
            saveCache()
        }
    }

    func restartStream() {
        streamTask?.cancel()
        guard pairing != nil, isActive else { return }
        streamTask = Task { [weak self] in await self?.runStream() }
    }

    private func runStream() async {
        var backoff: Double = 1
        while !Task.isCancelled {
            guard let pairing = SharedStore.orderedPairing else { return }
            if connection != .online { connection = .connecting }
            guard let client = await HostClient.resolve(pairing) else {
                connection = .offline(HostError.unreachable.localizedDescription)
                try? await Task.sleep(for: .seconds(backoff))
                backoff = min(backoff * 2, 20)
                continue
            }
            self.client = client
            SharedStore.preferredURL = client.baseURL.absoluteString
            do {
                if hello == nil || hello?.hostId != hostId {
                    let h = try await client.hello()
                    hello = h
                    // Host upgraded since the cache was written: its data may carry new fields.
                    if let stamp = cacheStamp, stamp != "\(Self.appBuild)/\(h.version)" {
                        rev = 0
                        cacheStamp = nil
                    }
                }
                for try await event in client.events(since: rev, client: "ios") {
                    connection = .online
                    backoff = 1
                    apply(event)
                }
            } catch is CancellationError {
                return
            } catch {
                if Task.isCancelled { return }
                log.info("stream ended: \(error.localizedDescription)")
                if case HostError.http(401, _) = error {
                    connection = .offline(error.localizedDescription)
                    return
                }
                connection = .offline(error.localizedDescription)
            }
            try? await Task.sleep(for: .seconds(backoff))
            backoff = min(backoff * 2, 20)
        }
    }

    private func apply(_ event: HostEvent) {
        switch event {
        case let .hello(id, hostRev, newUsage):
            if let hostId, hostId != id {
                // Different host database: start over.
                resetMirror()
            }
            hostId = id
            setUsage(newUsage)
            if hostRev < rev { rev = 0 }
        case let .bot(bot):
            bots[bot.id] = bot
            bump(bot.rev)
            LiveActivities.shared.update(bot: bot)
        case let .botDeleted(id, r):
            bots[id] = nil
            entries[id] = nil
            bump(r)
        case let .entry(e):
            upsert(e)
            bump(e.rev)
        case let .usage(u):
            setUsage(u)
        case .resync:
            restartStream()
        case let .undecodable(type):
            // Never skip past data we couldn't read: rewind once and fetch everything again.
            log.error("undecodable \(type) event")
            if !rewound {
                rewound = true
                rev = 0
                restartStream()
            }
        }
        scheduleSave()
    }

    private func bump(_ r: Int64) { rev = max(rev, r) }

    private func upsert(_ e: Entry) {
        var list = entries[e.botId] ?? []
        if let i = list.firstIndex(where: { $0.id == e.id }) {
            list[i] = e
        } else {
            // Replace the optimistic copy of a message we sent.
            if e.kind == "user", let nonce = e.data.clientNonce, !nonce.isEmpty {
                list.removeAll { $0.id == "local-\(nonce)" }
            }
            if let last = list.last, last.seq > e.seq, e.seq > 0 {
                let i = list.firstIndex { $0.seq > e.seq } ?? list.endIndex
                list.insert(e, at: i)
            } else {
                list.append(e)
            }
        }
        entries[e.botId] = list
    }

    private func setUsage(_ u: Usage) {
        guard u != usage else { return }
        usage = u
        SharedStore.usage = u
        WidgetCenter.shared.reloadAllTimelines()
    }

    // MARK: actions

    private func require() throws -> HostClient {
        guard let client, connection == .online else { throw HostError.unreachable }
        return client
    }

    func send(_ text: String, to botId: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        let nonce = UUID().uuidString
        let local = Entry(
            id: "local-\(nonce)", seq: Int64.max, botId: botId, rev: 0, kind: "user",
            turn: 0, data: EntryData(text: trimmed, status: "sending", clientNonce: nonce),
            createdAt: Int64(Date.now.timeIntervalSince1970 * 1000), updatedAt: 0
        )
        upsert(local)
        Task {
            do {
                let e = try await require().send(botId: botId, text: trimmed, clientNonce: nonce)
                upsert(e)
                if let bot = bots[botId] { LiveActivities.shared.start(for: bot, model: self) }
            } catch {
                markLocal(nonce: nonce, botId: botId, status: "failed")
            }
        }
    }

    func retry(_ entry: Entry) {
        guard let text = entry.data.text else { return }
        entries[entry.botId]?.removeAll { $0.id == entry.id }
        send(text, to: entry.botId)
    }

    func discard(_ entry: Entry) {
        entries[entry.botId]?.removeAll { $0.id == entry.id }
    }

    private func markLocal(nonce: String, botId: String, status: String) {
        guard var list = entries[botId], let i = list.firstIndex(where: { $0.id == "local-\(nonce)" }) else { return }
        list[i].data.status = status
        entries[botId] = list
    }

    func stop(_ botId: String) { perform { try await $0.stop(botId) } }
    func newSession(_ botId: String) { perform { try await $0.newSession(botId) } }

    func respond(_ entry: Entry, option: String?) {
        perform { try await $0.respondPermission(entryId: entry.id, optionId: option) }
    }

    func markRead(_ botId: String) {
        guard (bots[botId]?.unread ?? 0) > 0 else { return }
        bots[botId]?.unread = 0
        perform { try await $0.markRead(botId) }
    }

    func setPinned(_ bot: Bot, _ pinned: Bool) {
        var d = BotDraft(bot)
        d.pinned = pinned
        bots[bot.id]?.pinned = pinned
        perform { _ = try await $0.updateBot(d) }
    }

    func setHidden(_ bot: Bot, _ hidden: Bool) {
        var d = BotDraft(bot)
        d.hidden = hidden
        bots[bot.id]?.hidden = hidden
        perform { _ = try await $0.updateBot(d) }
    }

    func delete(_ bot: Bot) {
        bots[bot.id] = nil
        entries[bot.id] = nil
        perform { try await $0.deleteBot(bot.id) }
    }

    func save(_ draft: BotDraft) async throws -> Bot {
        let client = try require()
        let bot = draft.id == nil ? try await client.createBot(draft) : try await client.updateBot(draft)
        bots[bot.id] = bot
        return bot
    }

    func listDirs(_ path: String?) async throws -> DirListing {
        try await require().listDirs(path)
    }

    func refreshUsage() async {
        guard let client else { return }
        if let u = try? await client.usage(refresh: true) { setUsage(u) }
    }

    func loadOlder(_ botId: String) async {
        guard let client, !historyComplete.contains(botId) else { return }
        let first = thread(botId).first { $0.seq > 0 && $0.seq != Int64.max }?.seq ?? Int64.max
        guard let older = try? await client.history(botId: botId, beforeSeq: first, limit: 100) else { return }
        if older.count < 100 { historyComplete.insert(botId) }
        for e in older { upsert(e) }
    }

    private func perform(_ op: @escaping @MainActor (HostClient) async throws -> Void) {
        Task {
            do {
                try await op(try require())
            } catch {
                lastError = error.localizedDescription
            }
        }
    }

    // MARK: cache (instant launch, offline reading)

    private struct Cache: Codable {
        /// App build + host version that wrote the cache; any change means refetch everything.
        var stamp: String?
        var hostId: String?
        var rev: Int64
        var bots: [Bot]
        var entries: [Entry]
    }

    private static let appBuild = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "0"
    private var cacheStamp: String?

    private static var cacheURL: URL {
        URL.cachesDirectory.appending(path: "codync-mirror.json")
    }

    private func loadCache() {
        guard pairing != nil,
              let data = try? Data(contentsOf: Self.cacheURL),
              let cache = try? JSONDecoder().decode(Cache.self, from: data),
              cache.stamp?.hasPrefix(Self.appBuild + "/") == true else { return }
        cacheStamp = cache.stamp
        hostId = cache.hostId
        rev = cache.rev
        bots = Dictionary(cache.bots.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        entries = Dictionary(grouping: cache.entries, by: \.botId)
    }

    private func scheduleSave() {
        saveTask?.cancel()
        saveTask = Task {
            try? await Task.sleep(for: .seconds(2))
            if !Task.isCancelled { saveCache() }
        }
    }

    func saveCache() {
        // Keep the newest 200 entries per bot; older ones page in from the host.
        let kept = entries.values.flatMap { $0.filter { !$0.id.hasPrefix("local-") }.suffix(200) }
        let cache = Cache(stamp: "\(Self.appBuild)/\(hello?.version ?? "")", hostId: hostId, rev: rev, bots: Array(bots.values), entries: kept)
        if let data = try? JSONEncoder().encode(cache) {
            try? data.write(to: Self.cacheURL, options: .atomic)
        }
    }
}
