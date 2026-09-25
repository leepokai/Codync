import CodyncKit
import Foundation
import Observation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "BotStore")

/// Single source of truth for a Codync client (iPhone app, Mac window): mirrors
/// the host's bots and transcripts via one SSE stream (catch-up since `rev`, then live).
/// Opens the screen viewer, optionally watching a bot that's using the computer.
public struct ScreenRequest: Identifiable, Hashable, Sendable {
    public var watching: String?
    public var id: String { watching ?? "computer" }
    public init(watching: String? = nil) { self.watching = watching }
}

@MainActor
@Observable
public final class BotStore {
    public enum Connection: Equatable {
        case unpaired
        case connecting
        case online
        case offline(String)
    }

    public private(set) var pairing: Pairing?
    /// Computers this phone knows, most recently used first (the active one leads).
    public private(set) var computers: [Pairing] = []
    public private(set) var connection: Connection = .unpaired
    public private(set) var client: HostClient?
    public private(set) var hello: Hello?
    public private(set) var bots: [String: Bot] = [:]
    public private(set) var entries: [String: [Entry]] = [:]
    public private(set) var usage: Usage
    /// The computer's remote screen (`nil`: the host predates it).
    public private(set) var screen: ScreenState?
    /// Installed on the computer, for the Plugins screen and bot settings.
    public private(set) var installedConnectors: [InstalledConnector] = []
    public private(set) var installedSkills: [InstalledSkill] = []
    /// Bots whose older history has been fully paged in.
    public private(set) var historyComplete: Set<String> = []
    public var lastError: String?
    /// The open conversation (iOS navigation path / Mac sidebar selection).
    public var selection: String?
    /// The phone's profile sheet (computers, usage); `codync://computers` opens it.
    public var showProfile = false
    /// The Plugins screen on its own; `codync://plugins` opens it.
    public var showPlugins = false
    /// The remote screen viewer (iPhone); `codync://screen` opens it.
    public var screenRequest: ScreenRequest?

    // Platform hooks (push registration, Live Activities, widgets).
    public var onPaired: (@MainActor (BotStore) -> Void)?
    public var onBotUpdated: (@MainActor (Bot) -> Void)?
    public var onUsageChanged: (@MainActor (Usage) -> Void)?
    public var onSent: (@MainActor (Bot) -> Void)?

    /// `ios` clients suppress pushes while connected; others don't.
    private let clientKind: String
    /// Whether pairing lives in the shared App Group (iPhone) or is supplied each launch (Mac).
    private let persistsPairing: Bool
    public let storage: SharedStore.Context
    private var retired = false

    private var rev: Int64 = 0
    private var hostId: String?
    private var streamTask: Task<Void, Never>?
    private var isActive = true
    private var saveTask: Task<Void, Never>?
    private var rewound = false

    public init(pairing: Pairing?, clientKind: String, persistsPairing: Bool, storage: SharedStore.Context? = nil) {
        let storage = storage ?? (persistsPairing ? SharedStore.activeContext : SharedStore.Context(accountID: nil))
        self.storage = storage
        usage = storage.usage ?? Usage()
        self.pairing = pairing
        self.clientKind = clientKind
        self.persistsPairing = persistsPairing
        computers = persistsPairing ? storage.computers : pairing.map { [$0] } ?? []
        loadCache()
        connection = pairing == nil ? .unpaired : .connecting
    }

    // MARK: derived

    private var orderedPairing: Pairing? {
        guard persistsPairing else { return pairing }
        return storage.orderedPairing ?? pairing
    }

    public var isOffline: Bool {
        if case .offline = connection { true } else { false }
    }

    public var hostName: String { hello?.name ?? pairing?.name ?? "Computer" }

    /// Roster order: pinned first (manual order not tracked yet), then most recent activity.
    public var roster: [Bot] {
        bots.values.filter { !$0.hidden }.sorted {
            if $0.pinned != $1.pinned { return $0.pinned }
            return $0.lastAt > $1.lastAt
        }
    }

    /// Display name for a harness id, as the host reports it (falls back to the built-in list).
    public func backendName(_ id: String) -> String {
        hello?.backends.first { $0.id == id }?.name ?? BackendInfo.name(id)
    }

    public var hiddenBots: [Bot] { bots.values.filter(\.hidden).sorted { $0.name < $1.name } }

    public func thread(_ botId: String) -> [Entry] { entries[botId] ?? [] }

    // MARK: pairing

    public func pair(_ p: Pairing) {
        guard !retired else { return }
        let changedHost = pairing?.token != p.token
        pairing = p
        if persistsPairing {
            storage.pairing = p
            storage.preferredURL = nil
        }
        // Re-pairing the same computer (new token after a reset) replaces its old entry.
        computers = [p] + computers.filter { $0.token != p.token && $0.name != p.name }
        if persistsPairing { storage.computers = computers }
        if changedHost { resetMirror() }
        connection = .connecting
        restartStream()
        onPaired?(self)
    }

    /// Forgets a computer; forgetting the active one switches to the next, if any.
    public func forget(_ p: Pairing) {
        computers.removeAll { $0.token == p.token }
        if persistsPairing { storage.computers = computers }
        guard p.token == pairing?.token else { return }
        if let next = computers.first { pair(next) } else { unpair() }
    }

    public func setColor(_ p: Pairing, _ color: String) {
        updateComputer(p.token) { $0.color = color }
    }

    private func updateComputer(_ token: String, _ change: (inout Pairing) -> Void) {
        computers = computers.map { var c = $0; if c.token == token { change(&c) }; return c }
        if pairing?.token == token { change(&pairing!) }
        guard persistsPairing else { return }
        storage.computers = computers
        if storage.pairing?.token == token, var p = storage.pairing { change(&p); storage.pairing = p }
    }

    public func unpair() {
        streamTask?.cancel()
        pairing = nil
        client = nil
        hello = nil
        if persistsPairing { storage.pairing = nil }
        resetMirror()
        connection = .unpaired
    }

    private func resetMirror() {
        bots = [:]
        entries = [:]
        rev = 0
        hostId = nil
        historyComplete = []
        screen = nil
        saveCache()
    }

    // MARK: lifecycle

    /// Permanently detach a store when the client changes accounts. Existing
    /// async callbacks retain this old store and its fixed storage namespace.
    public func retire() {
        setActive(false)
        retired = true
        saveTask?.cancel()
        onPaired = nil
        onBotUpdated = nil
        onUsageChanged = nil
        onSent = nil
        client = nil
        selection = nil
        screenRequest = nil
        showProfile = false
        showPlugins = false
    }

    public func setActive(_ active: Bool) {
        guard !retired else { return }
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

    public func restartStream() {
        streamTask?.cancel()
        guard pairing != nil, isActive, !retired else { return }
        if connection != .online { connection = .connecting }
        streamTask = Task { [weak self] in await self?.runStream() }
    }

    private func runStream() async {
        var backoff: Double = 1
        while !Task.isCancelled {
            guard let pairing = orderedPairing else { return }
            guard let client = await HostClient.resolve(pairing) else {
                guard !Task.isCancelled, !retired else { return }
                connection = .offline(HostError.unreachable.localizedDescription)
                try? await Task.sleep(for: .seconds(backoff))
                backoff = min(backoff * 2, 20)
                continue
            }
            guard !Task.isCancelled, !retired else { return }
            self.client = client
            if persistsPairing { storage.preferredURL = client.baseURL.absoluteString }
            do {
                if hello == nil || hello?.hostId != hostId {
                    let h = try await client.hello()
                    guard !Task.isCancelled, !retired else { return }
                    hello = h
                    if let device = h.device, self.pairing?.device != device, let token = self.pairing?.token {
                        updateComputer(token) { $0.device = device }
                    }
                    // Addresses the host gained since pairing (say, Tailscale installed later) join the list.
                    if let urls = h.urls, let token = self.pairing?.token,
                       let known = self.pairing?.urls, !Set(urls).isSubset(of: Set(known)) {
                        updateComputer(token) { p in p.urls += urls.filter { !p.urls.contains($0) } }
                    }
                    // Host upgraded since the cache was written: its data may carry new fields.
                    if let stamp = cacheStamp, stamp != "\(Self.appBuild)/\(h.version)" {
                        rev = 0
                        cacheStamp = nil
                    }
                }
                for try await event in client.events(since: rev, client: clientKind) {
                    guard !Task.isCancelled, !retired else { return }
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
                // A dropped or timed-out stream means the computer went away; say so plainly.
                connection = .offline(error is URLError ? HostError.unreachable.localizedDescription : error.localizedDescription)
            }
            try? await Task.sleep(for: .seconds(backoff))
            backoff = min(backoff * 2, 20)
        }
    }

    private func apply(_ event: HostEvent) {
        switch event {
        case let .hello(id, hostRev, newUsage, newScreen):
            if let hostId, hostId != id {
                // Different host database: start over.
                resetMirror()
            }
            hostId = id
            setUsage(newUsage)
            screen = newScreen
            if hostRev < rev { rev = 0 }
        case let .bot(bot):
            bots[bot.id] = bot
            bump(bot.rev)
            onBotUpdated?(bot)
        case let .botDeleted(id, r):
            bots[id] = nil
            entries[id] = nil
            bump(r)
        case let .entry(e):
            upsert(e)
            bump(e.rev)
        case let .usage(u):
            setUsage(u)
        case let .screen(s):
            screen = s
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
            // A late API response mustn't undo a newer SSE update.
            guard e.rev >= list[i].rev else { return }
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
        onUsageChanged?(u)
    }

    // MARK: actions

    private func require() throws -> HostClient {
        guard !retired, let client, connection == .online else { throw HostError.unreachable }
        return client
    }

    public func send(_ text: String, to botId: String) {
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
                if let bot = bots[botId] { onSent?(bot) }
            } catch {
                markLocal(nonce: nonce, botId: botId, status: "failed")
            }
        }
    }

    public func retry(_ entry: Entry) {
        guard let text = entry.data.text else { return }
        entries[entry.botId]?.removeAll { $0.id == entry.id }
        send(text, to: entry.botId)
    }

    public func discard(_ entry: Entry) {
        entries[entry.botId]?.removeAll { $0.id == entry.id }
    }

    private func markLocal(nonce: String, botId: String, status: String) {
        guard var list = entries[botId], let i = list.firstIndex(where: { $0.id == "local-\(nonce)" }) else { return }
        list[i].data.status = status
        entries[botId] = list
    }

    public func stop(_ botId: String) { perform { try await $0.stop(botId) } }
    public func newSession(_ botId: String) { perform { try await $0.newSession(botId) } }

    // MARK: remote screen

    /// Takes control from bots (they can still look) or hands it back.
    public func screenTakeover(_ on: Bool) {
        perform { [weak self] in self?.screen = try await $0.screenTakeover(on) }
    }

    /// Only the computer itself may turn remote screen on (the Mac menu).
    public func setScreenEnabled(_ on: Bool) async throws {
        screen = try await require().setScreenEnabled(on)
    }

    public func respond(_ entry: Entry, option: String?) {
        perform { try await $0.respondPermission(entryId: entry.id, optionId: option) }
    }

    public func markRead(_ botId: String) {
        guard (bots[botId]?.unread ?? 0) > 0 else { return }
        bots[botId]?.unread = 0
        perform { try await $0.markRead(botId) }
    }

    public func setPinned(_ bot: Bot, _ pinned: Bool) {
        var d = BotDraft(bot)
        d.pinned = pinned
        bots[bot.id]?.pinned = pinned
        perform { _ = try await $0.updateBot(d) }
    }

    public func setHidden(_ bot: Bot, _ hidden: Bool) {
        var d = BotDraft(bot)
        d.hidden = hidden
        bots[bot.id]?.hidden = hidden
        perform { _ = try await $0.updateBot(d) }
    }

    public func delete(_ bot: Bot) {
        bots[bot.id] = nil
        entries[bot.id] = nil
        perform { try await $0.deleteBot(bot.id) }
    }

    public func save(_ draft: BotDraft) async throws -> Bot {
        let client = try require()
        let bot = draft.id == nil ? try await client.createBot(draft) : try await client.updateBot(draft)
        bots[bot.id] = bot
        return bot
    }

    // MARK: plugins

    public func refreshPlugins() async {
        guard let client else { return }
        async let c = client.connectors()
        async let s = client.skills()
        if let c = try? await c { installedConnectors = c }
        if let s = try? await s { installedSkills = s }
    }

    public func marketConnectors(search: String) async throws -> [MarketConnector] {
        try await require().marketConnectors(search: search)
    }

    public func marketSkills() async throws -> [MarketSkill] {
        try await require().marketSkills()
    }

    public func installConnector(_ item: MarketConnector, option: String, inputs: [String: String]) async throws {
        try await require().installConnector(registryName: item.name, option: option, inputs: inputs)
        await refreshPlugins()
    }

    public func addConnector(name: String, command: String?, url: String?, env: [String: String]) async throws {
        try await require().addConnector(name: name, command: command, url: url, env: env)
        await refreshPlugins()
    }

    public func removeConnector(_ id: String) async throws {
        try await require().removeConnector(id)
        await refreshPlugins()
    }

    public func installSkill(source: String) async throws {
        try await require().installSkill(source: source)
        await refreshPlugins()
    }

    public func addSkill(name: String, description: String, instructions: String) async throws {
        try await require().addSkill(name: name, description: description, instructions: instructions)
        await refreshPlugins()
    }

    public func removeSkill(_ id: String) async throws {
        try await require().removeSkill(id)
        await refreshPlugins()
    }

    /// Re-detects agents on the computer (after an install or sign-in).
    public func refreshBackends() async {
        guard let client, let backends = try? await client.refreshBackends() else { return }
        hello?.backends = backends
    }

    public func listDirs(_ path: String?) async throws -> DirListing {
        try await require().listDirs(path)
    }

    public func refreshUsage() async {
        guard let client else { return }
        if let u = try? await client.usage(refresh: true) { setUsage(u) }
    }

    public func loadOlder(_ botId: String) async {
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

    private var cacheURL: URL {
        let computer = SharedStore.Context.digest(pairing?.token ?? "unpaired")
        return URL.cachesDirectory.appending(path: "codync-mirror-\(Bundle.main.bundleIdentifier ?? "app")-\(storage.id)-\(computer).json")
    }

    private func loadCache() {
        guard pairing != nil,
              let data = try? Data(contentsOf: cacheURL),
              let cache = try? JSONDecoder().decode(Cache.self, from: data),
              cache.stamp?.hasPrefix(Self.appBuild + "/") == true else { return }
        cacheStamp = cache.stamp
        hostId = cache.hostId
        rev = cache.rev
        bots = Dictionary(cache.bots.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        entries = Dictionary(grouping: cache.entries, by: \.botId)
    }

    private func scheduleSave() {
        guard !retired else { return }
        saveTask?.cancel()
        saveTask = Task {
            try? await Task.sleep(for: .seconds(2))
            if !Task.isCancelled { saveCache() }
        }
    }

    public func saveCache() {
        guard !retired else { return }
        // Keep the newest 200 entries per bot; older ones page in from the host.
        let kept = entries.values.flatMap { $0.filter { !$0.id.hasPrefix("local-") }.suffix(200) }
        let cache = Cache(stamp: "\(Self.appBuild)/\(hello?.version ?? "")", hostId: hostId, rev: rev, bots: Array(bots.values), entries: kept)
        if let data = try? JSONEncoder().encode(cache) {
            try? data.write(to: cacheURL, options: .atomic)
        }
    }
}
