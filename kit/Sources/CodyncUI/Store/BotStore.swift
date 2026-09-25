import CodyncKit
import Foundation
import Observation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "BotStore")

/// Opens the screen viewer, optionally watching a bot that's using the computer.
public struct ScreenRequest: Identifiable, Hashable, Sendable {
    public var watching: String?
    public var id: String { watching ?? "computer" }
    public init(watching: String? = nil) { self.watching = watching }
}

/// One computer's mirror (bots, transcripts) for a client (iPhone app, Mac window), kept by one
/// events stream (catch-up since `rev`, then live) over loopback or the encrypted channel.
/// `AccountStore` holds one per computer.
@MainActor
@Observable
public final class BotStore {
    public enum Connection: Equatable {
        case unpaired
        case connecting
        case online
        /// The relay says the computer isn't connected; messages can wait in its mailbox.
        case computerOffline(lastSeen: Date?)
        /// Can't reach the computer at all (no direct route, no relay).
        case offline(String)
        /// Revoked, lease expired, or the computer's identity changed.
        case unauthorized(String)
    }

    /// How this store reaches its computer.
    public enum Route: Sendable, Hashable {
        /// The Mac's own host, or one through an SSH tunnel.
        case loopback(baseURL: URL, token: String)
        /// The end-to-end encrypted channel (direct or relay), with this context's device key.
        case channel
    }

    public private(set) var computer: Computer
    public let route: Route
    public private(set) var connection: Connection = .connecting
    public private(set) var hostRoute: HostRoute?
    public private(set) var client: HostClient?
    public private(set) var hello: Hello?
    public private(set) var bots: [String: Bot] = [:]
    public private(set) var entries: [String: [Entry]] = [:]
    public private(set) var usage: Usage
    /// The computer's remote screen (`nil`: the host predates it).
    public private(set) var screen: ScreenState?
    /// Devices asking this computer for access (loopback only: the Mac approves them).
    public private(set) var accessRequests: [AccessRequest] = []
    /// The host's cloud connection (loopback only).
    public private(set) var cloud: CloudStatus?
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
    /// Each time the channel (or loopback) becomes ready.
    public var onConnected: (@MainActor (BotStore) -> Void)?
    public var onBotUpdated: (@MainActor (Bot) -> Void)?
    public var onUsageChanged: (@MainActor (ComputerID, Usage) -> Void)?
    public var onSent: (@MainActor (Bot) -> Void)?
    /// A bot appeared, changed or went away.
    var onRosterChanged: (@MainActor () -> Void)?
    /// The computer's addresses or keys changed (merged from its `hello`); persist it.
    var onComputerChanged: (@MainActor (Computer) -> Void)?

    /// `ios` clients suppress pushes while connected; others don't.
    private let clientKind: String
    public let storage: SharedStore.Context
    private let makeTransport: @MainActor () async throws -> any HostTransport
    private var transport: (any HostTransport)?
    private var retired = false

    private var rev: Int64 = 0
    private var hostId: String?
    private var streamTask: Task<Void, Never>?
    private var eventsTask: Task<Void, Never>?
    private var isActive = true
    private var saveTask: Task<Void, Never>?
    private var rewound = false

    /// `.channel` loads this context's `DeviceIdentity` itself.
    public convenience init(computer: Computer, route: Route, clientKind: String, storage: SharedStore.Context) {
        let make: @MainActor () async throws -> any HostTransport = switch route {
        case let .loopback(baseURL, token):
            { LoopbackTransport(baseURL: baseURL, token: token) }
        case .channel:
            { try await HostConnector.connect(computer, identity: try DeviceIdentity.load(context: storage)) }
        }
        self.init(computer: computer, route: route, clientKind: clientKind, storage: storage, transport: make)
    }

    init(computer: Computer, route: Route, clientKind: String, storage: SharedStore.Context,
         transport: @escaping @MainActor () async throws -> any HostTransport) {
        self.computer = computer
        self.route = route
        self.clientKind = clientKind
        self.storage = storage
        makeTransport = transport
        usage = storage.usage[computer.id] ?? Usage()
        loadCache()
    }

    // MARK: derived

    /// Offline in any way: the computer is off, unreachable, or refuses this device.
    public var isOffline: Bool {
        switch connection {
        case .computerOffline, .offline, .unauthorized: true
        default: false
        }
    }

    /// Offline, but sends can wait in the relay mailbox until the computer is back.
    public var canQueue: Bool {
        if case .computerOffline = connection { computer.boxKey != nil && transport is any RemoteTransport } else { false }
    }

    public var hostName: String { hello?.name ?? computer.name }

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

    func updateComputer(_ change: (inout Computer) -> Void) {
        var c = computer
        change(&c)
        guard c != computer else { return }
        computer = c
        onComputerChanged?(c)
    }

    private func resetMirror() {
        bots = [:]
        entries = entries.mapValues { $0.filter { $0.id.hasPrefix("local-") } }.filter { !$0.value.isEmpty }
        selection = nil
        onRosterChanged?()
        rev = 0
        hostId = nil
        historyComplete = []
        screen = nil
        saveCache()
    }

    // MARK: lifecycle

    /// Permanently detach a store (account switch, computer removed). Late async
    /// callbacks still hold it, but it writes nothing and calls no hooks anymore.
    public func retire() {
        setActive(false)
        retired = true
        saveTask?.cancel()
        onConnected = nil
        onBotUpdated = nil
        onUsageChanged = nil
        onSent = nil
        onRosterChanged = nil
        onComputerChanged = nil
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
            stopTransport()
            saveCache()
        }
    }

    public func restartStream() {
        guard isActive, !retired else { return }
        stopTransport()
        if connection != .online { connection = .connecting }
        streamTask = Task { [weak self] in await self?.runStream() }
    }

    private func stopTransport() {
        streamTask?.cancel()
        streamTask = nil
        eventsTask?.cancel()
        eventsTask = nil
        if let remote = transport as? any RemoteTransport {
            Task { await remote.shutdown() }
        }
        transport = nil
    }

    private func runStream() async {
        var backoff: Double = 1
        var transport: any HostTransport
        while true {
            do {
                transport = try await makeTransport()
                break
            } catch HostError.unauthorized(let message) {
                if !Task.isCancelled, !retired { connection = .unauthorized(message) }
                return
            } catch {
                guard !Task.isCancelled, !retired else { return }
                connection = .offline(error.localizedDescription)
                try? await Task.sleep(for: .seconds(backoff))
                backoff = min(backoff * 2, 30)
                guard !Task.isCancelled else { return }
            }
        }
        guard !Task.isCancelled, !retired else {
            if let remote = transport as? any RemoteTransport { await remote.shutdown() }
            return
        }
        self.transport = transport
        let client = HostClient(transport: transport)
        self.client = client
        if let remote = transport as? any RemoteTransport { watch(remote) }
        for await state in transport.states() {
            guard !Task.isCancelled, !retired else { return }
            switch state {
            case let .ready(route):
                hostRoute = route
                if eventsTask == nil {
                    if connection != .online { connection = .connecting }
                    eventsTask = Task { [weak self] in await self?.runEvents(client) }
                    // Mailbox outcomes that happened while this app wasn't listening (§10.2).
                    if let remote = transport as? any RemoteTransport {
                        Task { [weak self] in await self?.reconcileQueued(remote) }
                    }
                }
                onConnected?(self)
            case .connecting:
                stopEvents()
                connection = .connecting
            case let .hostOffline(lastSeen):
                stopEvents()
                connection = .computerOffline(lastSeen: lastSeen)
                if let remote = transport as? any RemoteTransport {
                    Task { [weak self] in await self?.reconcileQueued(remote) }
                }
            case let .unauthorized(message):
                stopEvents()
                connection = .unauthorized(message)
            case let .failed(message):
                stopEvents()
                connection = .offline(message)
            }
        }
    }

    private func stopEvents() {
        eventsTask?.cancel()
        eventsTask = nil
        hostRoute = nil
    }

    /// The channel's side streams: merged computer info and mailbox outcomes.
    private func watch(_ remote: any RemoteTransport) {
        let updates = remote.computerUpdates()
        let mailbox = remote.mailboxEvents()
        Task { [weak self] in
            for await computer in updates {
                guard let self, !self.retired else { return }
                // The color is picked on this device; the transport's copy may be older.
                self.updateComputer { current in
                    let color = current.color
                    current = computer
                    current.color = color
                }
            }
        }
        Task { [weak self] in
            for await event in mailbox {
                guard let self, !self.retired else { return }
                self.applyMailbox(event)
            }
        }
    }

    /// Events (catch-up since `rev`, then live) for as long as the link stays ready.
    private func runEvents(_ client: HostClient) async {
        var backoff: Double = 1
        while !Task.isCancelled {
            do {
                if hello == nil || hello?.hostId != hostId {
                    let h = try await client.hello()
                    guard !Task.isCancelled, !retired else { return }
                    hello = h
                    if case .loopback = route {
                        updateComputer { c in
                            c.name = h.name
                            if let device = h.device { c.device = device }
                        }
                    }
                    // Host upgraded since the cache was written: its data may carry new fields.
                    if let stamp = cacheStamp, stamp != "\(Self.appBuild)/\(h.version)" {
                        rev = 0
                        cacheStamp = nil
                    }
                }
                if case .loopback = route {
                    accessRequests = (try? await client.accessRequests()) ?? accessRequests
                    cloud = (try? await client.cloudStatus()) ?? cloud
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
                if Task.isCancelled || retired { return }
                log.info("stream ended: \(error.localizedDescription)")
                if case let HostError.unauthorized(message) = error {
                    connection = .unauthorized(message)
                    return
                }
                // On the channel the link state speaks for itself; loopback has no other signal.
                if case .loopback = route {
                    if case HostError.http(401, _) = error {
                        connection = .offline(error.localizedDescription)
                        return
                    }
                    connection = .offline(error.localizedDescription)
                }
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
            onRosterChanged?()
        case let .botDeleted(id, r):
            bots[id] = nil
            entries[id] = nil
            bump(r)
            if selection == id { selection = nil }
            onRosterChanged?()
        case let .entry(e):
            upsert(e)
            bump(e.rev)
        case let .usage(u):
            setUsage(u)
        case let .screen(s):
            screen = s
        case let .accessRequests(requests):
            accessRequests = requests
        case let .cloud(status):
            cloud = status
        case .resync:
            restartEvents()
        case let .undecodable(type):
            // Never skip past data we couldn't read: rewind once and fetch everything again.
            log.error("undecodable \(type) event")
            if !rewound {
                rewound = true
                rev = 0
                restartEvents()
            }
        }
        scheduleSave()
    }

    /// Resubscribes from the current `rev` on the same link.
    private func restartEvents() {
        guard let client, eventsTask != nil else { return }
        eventsTask?.cancel()
        eventsTask = Task { [weak self] in await self?.runEvents(client) }
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
        guard u != usage, !retired else { return }
        usage = u
        storage.usage[computer.id] = u
        onUsageChanged?(computer.id, u)
    }

    // MARK: actions

    private func require() throws -> HostClient {
        guard !retired, let client, connection == .online else { throw HostError.unreachable }
        return client
    }

    public func send(_ text: String, to botId: String) {
        send(text, to: botId, nonce: UUID().uuidString)
    }

    private func send(_ text: String, to botId: String, nonce: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !retired else { return }
        let local = Entry(
            id: "local-\(nonce)", seq: Int64.max, botId: botId, rev: 0, kind: "user",
            turn: 0, data: EntryData(text: trimmed, status: "sending", clientNonce: nonce),
            createdAt: Int64(Date.now.timeIntervalSince1970 * 1000), updatedAt: 0
        )
        upsert(local)
        storage.lastComputerId = computer.id
        if canQueue, let remote = transport as? any RemoteTransport {
            enqueue(remote, text: trimmed, botId: botId, nonce: nonce)
            return
        }
        deliver(trimmed, botId: botId, nonce: nonce)
    }

    private func deliver(_ text: String, botId: String, nonce: String) {
        Task {
            do {
                let e = try await require().send(botId: botId, text: text, clientNonce: nonce)
                upsert(e)
                if let bot = bots[botId] { onSent?(bot) }
            } catch {
                markLocal(nonce: nonce, botId: botId, status: "failed")
            }
        }
    }

    /// The computer is offline: the message waits, sealed for it, in the relay mailbox.
    private func enqueue(_ remote: any RemoteTransport, text: String, botId: String, nonce: String) {
        markLocal(nonce: nonce, botId: botId, status: "waiting")
        saveCache()
        Task {
            do {
                try await remote.enqueue(botId: botId, text: text, clientNonce: nonce)
                if let bot = bots[botId] { onSent?(bot) }
            } catch MailboxError.hostOnline {
                // It came back meanwhile: send it the normal way once the link is up.
                markLocal(nonce: nonce, botId: botId, status: "sending")
                try? await Task.sleep(for: .seconds(2))
                deliver(text, botId: botId, nonce: nonce)
            } catch {
                lastError = error.localizedDescription
                markLocal(nonce: nonce, botId: botId, status: "failed")
            }
            saveCache()
        }
    }

    /// Takes a waiting message back out of the mailbox, unless the computer already has it.
    public func cancelQueued(_ entry: Entry) {
        guard let nonce = entry.data.clientNonce, let remote = transport as? any RemoteTransport else { return }
        Task {
            switch await remote.cancelQueued(clientNonce: nonce) {
            case .cancelled:
                discard(entry)
            case .delivering:
                markLocal(nonce: nonce, botId: entry.botId, status: "delivering")
            case .unknown:
                lastError = "Couldn't take the message back. It may have been delivered already."
            }
            saveCache()
        }
    }

    private func applyMailbox(_ event: MailboxEvent) {
        guard let entry = entries.values.lazy.flatMap({ $0 }).first(where: { $0.id == "local-\(event.nonce)" }) else { return }
        switch event {
        // The events stream's own copy replaces it (matched by clientNonce).
        case .delivered: markLocal(nonce: event.nonce, botId: entry.botId, status: "delivering")
        case .failed, .expired: markLocal(nonce: event.nonce, botId: entry.botId, status: "failed")
        }
        saveCache()
    }

    /// Messages the relay no longer holds were delivered, refused or expired while we were away.
    /// "Failed" is safe even if one was delivered: the events catch-up replaces it by clientNonce,
    /// and a resend reuses that nonce, so the computer never runs it twice.
    private func reconcileQueued(_ remote: any RemoteTransport) async {
        let waiting = entries.values.flatMap { $0 }.filter { $0.data.status == "waiting" }
        guard !waiting.isEmpty else { return }
        let held = Dictionary(await remote.listQueued().map { ($0.nonce, $0.state) }, uniquingKeysWith: { a, _ in a })
        guard !retired else { return }
        for e in waiting {
            guard let nonce = e.data.clientNonce else { continue }
            switch held[nonce] {
            case "queued": break
            case .some: markLocal(nonce: nonce, botId: e.botId, status: "delivering")
            case nil: markLocal(nonce: nonce, botId: e.botId, status: "failed")
            }
        }
        saveCache()
    }

    public func retry(_ entry: Entry) {
        guard let text = entry.data.text else { return }
        entries[entry.botId]?.removeAll { $0.id == entry.id }
        // The same clientNonce: if the computer got it after all, it won't run twice (the host
        // skips a nonce it has). Every seal still takes a fresh ephemeral key (§6.4).
        send(text, to: entry.botId, nonce: entry.data.clientNonce ?? UUID().uuidString)
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
        guard !retired, let client else { return }
        if let u = try? await client.usage(refresh: true), !Task.isCancelled { setUsage(u) }
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

    /// Per app, context and computer (`SharedStore.Context.erase()` removes a context's files).
    private var cacheURL: URL {
        URL.cachesDirectory.appending(path: "codync-mirror-\(Bundle.main.bundleIdentifier ?? "app")-\(storage.id)-\(computer.id).json")
    }

    private func loadCache() {
        guard let data = try? Data(contentsOf: cacheURL),
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
        // Keep the newest 200 entries per bot; older ones page in from the host. Messages waiting
        // in the mailbox stay too, so they can still be seen and cancelled after a relaunch.
        let kept = entries.values.flatMap { list in
            list.filter { !$0.id.hasPrefix("local-") }.suffix(200)
                + list.filter { $0.id.hasPrefix("local-") && ["waiting", "delivering"].contains($0.data.status) }
        }
        let cache = Cache(stamp: "\(Self.appBuild)/\(hello?.version ?? "")", hostId: hostId, rev: rev, bots: Array(bots.values), entries: kept)
        if let data = try? JSONEncoder().encode(cache) {
            try? data.write(to: cacheURL, options: .atomic)
        }
    }
}
