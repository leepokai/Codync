import Foundation

/// What `codync-host pair` encodes: `codync://pair?name=…&token=…&urls=a,b`.
public struct Pairing: Codable, Hashable, Sendable {
    public var name: String
    public var token: String
    public var urls: [String]

    public init(name: String, token: String, urls: [String]) {
        self.name = name
        self.token = token
        self.urls = urls
    }

    public init?(url: URL) {
        guard url.scheme == "codync", url.host() == "pair",
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems else { return nil }
        let q = Dictionary(items.map { ($0.name, $0.value ?? "") }, uniquingKeysWith: { a, _ in a })
        guard let token = q["token"], !token.isEmpty else { return nil }
        let urls = (q["urls"] ?? "").split(separator: ",").map(String.init).filter { URL(string: $0) != nil }
        guard !urls.isEmpty else { return nil }
        self.init(name: q["name"].flatMap { $0.isEmpty ? nil : $0 } ?? "My computer", token: token, urls: urls)
    }

    public init?(string: String) {
        guard let url = URL(string: string.trimmingCharacters(in: .whitespacesAndNewlines)) else { return nil }
        self.init(url: url)
    }
}

public enum HostError: LocalizedError, Sendable {
    case http(Int, String)
    case unreachable

    public var errorDescription: String? {
        switch self {
        case .http(401, _): "This phone isn't paired with the host anymore. Pair again."
        case let .http(_, message): message
        case .unreachable: "Can't reach your computer. Is it awake and on Tailscale or the same Wi-Fi?"
        }
    }
}

public enum HostEvent: Sendable {
    case hello(hostId: String, rev: Int64, usage: Usage)
    case bot(Bot)
    case botDeleted(id: String, rev: Int64)
    case entry(Entry)
    case usage(Usage)
    case resync
    /// An event this app version couldn't decode (host newer/older than the app).
    case undecodable(type: String)
}

public struct Empty: Codable, Sendable {
    public init() {}
}

public struct HostClient: Sendable {
    public let baseURL: URL
    public let token: String

    public init(baseURL: URL, token: String) {
        self.baseURL = baseURL
        self.token = token
    }

    static let decoder = JSONDecoder()
    static let encoder = JSONEncoder()

    private var session: URLSession { .shared }

    public func call<T: Decodable>(_ method: String, _ body: some Encodable = Empty(), timeout: TimeInterval = 20) async throws -> T {
        var req = URLRequest(url: baseURL.appending(path: "api/\(method)"), timeoutInterval: timeout)
        req.httpMethod = "POST"
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try Self.encoder.encode(body)
        let (data, response) = try await session.data(for: req)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status) else {
            let message = (try? JSONDecoder().decode([String: String].self, from: data))?["error"] ?? "Host error \(status)"
            throw HostError.http(status, message)
        }
        return try Self.decoder.decode(T.self, from: data)
    }

    public func healthy(timeout: TimeInterval = 4) async -> Bool {
        var req = URLRequest(url: baseURL.appending(path: "health"), timeoutInterval: timeout)
        req.cachePolicy = .reloadIgnoringLocalCacheData
        guard let (_, response) = try? await session.data(for: req) else { return false }
        return (response as? HTTPURLResponse)?.statusCode == 200
    }

    /// First address (in the pairing's preference order) that answers `/health`.
    public static func resolve(_ pairing: Pairing) async -> HostClient? {
        let candidates = pairing.urls.compactMap(URL.init(string:)).map { HostClient(baseURL: $0, token: pairing.token) }
        return await withTaskGroup(of: (Int, Bool).self) { group in
            for (i, c) in candidates.enumerated() {
                group.addTask { (i, await c.healthy()) }
            }
            var ok: [Int] = []
            for await (i, healthy) in group where healthy {
                ok.append(i)
                // The best-ranked address won; no need to wait for slower ones.
                if i == 0 { break }
            }
            group.cancelAll()
            return ok.min().map { candidates[$0] }
        }
    }

    /// Server-sent events: catch-up since `since`, then live updates.
    public func events(since: Int64, client: String) -> AsyncThrowingStream<HostEvent, Error> {
        var comps = URLComponents(url: baseURL.appending(path: "events"), resolvingAgainstBaseURL: false)!
        comps.queryItems = [.init(name: "since", value: String(since)), .init(name: "client", value: client)]
        var req = URLRequest(url: comps.url!, timeoutInterval: 60 * 60 * 24)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        let session = self.session
        let request = req
        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let (bytes, response) = try await session.bytes(for: request)
                    let status = (response as? HTTPURLResponse)?.statusCode ?? 0
                    guard status == 200 else { throw HostError.http(status, "Host error \(status)") }
                    for try await line in bytes.lines {
                        guard line.hasPrefix("data:") else { continue }
                        let payload = Data(line.dropFirst(5).utf8)
                        if let event = Self.parseEvent(payload) {
                            continuation.yield(event)
                        }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    public static func parseEvent(_ data: Data) -> HostEvent? {
        struct Envelope: Decodable {
            var type: String
            var hostId: String?
            var rev: Int64?
        }
        struct BotWrap: Decodable { var bot: Bot }
        struct StubWrap: Decodable { var bot: BotStub }
        struct EntryWrap: Decodable { var entry: Entry }
        struct UsageWrap: Decodable { var usage: Usage }
        let d = decoder
        guard let env = try? d.decode(Envelope.self, from: data) else { return nil }
        switch env.type {
        case "hello":
            let usage = (try? d.decode(UsageWrap.self, from: data))?.usage ?? Usage()
            return .hello(hostId: env.hostId ?? "", rev: env.rev ?? 0, usage: usage)
        case "bot":
            if let stub = try? d.decode(StubWrap.self, from: data), stub.bot.deleted == true {
                return .botDeleted(id: stub.bot.id, rev: stub.bot.rev)
            }
            return (try? d.decode(BotWrap.self, from: data)).map { .bot($0.bot) } ?? .undecodable(type: "bot")
        case "entry":
            return (try? d.decode(EntryWrap.self, from: data)).map { .entry($0.entry) } ?? .undecodable(type: "entry")
        case "usage":
            return (try? d.decode(UsageWrap.self, from: data)).map { .usage($0.usage) }
        case "resync":
            return .resync
        default:
            return nil
        }
    }
}

// MARK: - Typed calls

public extension HostClient {
    func hello() async throws -> Hello { try await call("hello") }

    func sync(since: Int64) async throws -> SyncResponse {
        try await call("sync", ["since": since])
    }

    func history(botId: String, beforeSeq: Int64, limit: Int = 100) async throws -> [Entry] {
        struct Body: Encodable { var botId: String; var beforeSeq: Int64; var limit: Int }
        struct Res: Decodable { var entries: [Entry] }
        let res: Res = try await call("history", Body(botId: botId, beforeSeq: beforeSeq, limit: limit))
        return res.entries
    }

    func createBot(_ draft: BotDraft) async throws -> Bot {
        struct Res: Decodable { var bot: Bot }
        let res: Res = try await call("createBot", draft)
        return res.bot
    }

    func updateBot(_ draft: BotDraft) async throws -> Bot {
        struct Res: Decodable { var bot: Bot }
        let res: Res = try await call("updateBot", draft)
        return res.bot
    }

    func deleteBot(_ id: String) async throws {
        let _: Empty = try await call("deleteBot", ["botId": id])
    }

    func markRead(_ id: String) async throws {
        let _: Empty = try await call("markRead", ["botId": id])
    }

    func send(botId: String, text: String, clientNonce: String) async throws -> Entry {
        struct Res: Decodable { var entry: Entry }
        let res: Res = try await call("send", ["botId": botId, "text": text, "clientNonce": clientNonce])
        return res.entry
    }

    func stop(_ botId: String) async throws {
        let _: Empty = try await call("stop", ["botId": botId])
    }

    func newSession(_ botId: String) async throws {
        let _: Empty = try await call("newSession", ["botId": botId])
    }

    func respondPermission(entryId: String, optionId: String?) async throws {
        struct Body: Encodable { var entryId: String; var optionId: String? }
        let _: Empty = try await call("respondPermission", Body(entryId: entryId, optionId: optionId))
    }

    func registerDevice(ticket: String, relay: String, name: String) async throws {
        let _: Empty = try await call("registerDevice", ["ticket": ticket, "relay": relay, "name": name])
    }

    func registerActivity(botId: String, ticket: String) async throws {
        let _: Empty = try await call("registerActivity", ["botId": botId, "ticket": ticket])
    }

    func usage(refresh: Bool = false) async throws -> Usage {
        try await call("usage", ["refresh": refresh])
    }

    /// Pairing link + candidate addresses for showing a QR code on this computer.
    func pairing() async throws -> PairingInfo { try await call("pairing") }

    func listDirs(_ path: String?) async throws -> DirListing {
        struct Body: Encodable { var path: String? }
        return try await call("listDirs", Body(path: path))
    }
}
