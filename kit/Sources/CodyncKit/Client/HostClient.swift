import Foundation

public enum HostEvent: Sendable {
    case hello(hostId: String, rev: Int64, usage: Usage, screen: ScreenState?)
    case bot(Bot)
    case botDeleted(id: String, rev: Int64)
    case entry(Entry)
    case usage(Usage)
    case screen(ScreenState)
    /// Loopback only: devices asking this computer for access.
    case accessRequests([AccessRequest])
    /// Loopback only: the host's cloud connection.
    case cloud(CloudStatus)
    case resync
    /// An event this app version couldn't decode (host newer/older than the app).
    case undecodable(type: String)
}

/// A setup terminal's output stream.
public enum TermEvent: Sendable {
    case output(Data)
    case exit(Int)
}

public struct Empty: Codable, Sendable {
    public init() {}
}

/// A full editor update must explicitly clear a previous model override.
struct BotUpdate: Encodable {
    let draft: BotDraft
    private enum CodingKeys: String, CodingKey { case model }

    func encode(to encoder: Encoder) throws {
        try draft.encode(to: encoder)
        if draft.model == nil {
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encodeNil(forKey: .model)
        }
    }
}

public struct HostClient: Sendable {
    public let transport: any HostTransport

    public init(transport: any HostTransport) {
        self.transport = transport
    }

    /// Loopback: the Mac's own host, or one reached through an SSH tunnel.
    public init(baseURL: URL, token: String) {
        self.init(transport: LoopbackTransport(baseURL: baseURL, token: token))
    }

    static let decoder = JSONDecoder()
    static let encoder = JSONEncoder()

    public func call<T: Decodable>(_ method: String, _ body: some Encodable = Empty(), timeout: TimeInterval = 20) async throws -> T {
        let data = try await transport.call(method, body: try Self.encoder.encode(body), timeout: timeout)
        return try Self.decoder.decode(T.self, from: data)
    }

    /// Catch-up since `since`, then live updates.
    public func events(since: Int64, client: String) -> AsyncThrowingStream<HostEvent, Error> {
        map(transport.stream(.events(since: since, client: client)), Self.parseEvent)
    }

    /// A setup terminal's output: everything so far, then live, finishing after `.exit`.
    public func termOutput(_ term: String) -> AsyncThrowingStream<TermEvent, Error> {
        map(transport.stream(.term(term))) { data in
            struct Raw: Decodable { var type: String; var data: String?; var code: Int? }
            guard let raw = try? Self.decoder.decode(Raw.self, from: data) else { return nil }
            switch raw.type {
            case "output": return raw.data.flatMap { Data(base64Encoded: $0) }.map(TermEvent.output)
            case "exit": return .exit(raw.code ?? -1)
            default: return nil
            }
        }
    }

    private func map<T: Sendable>(_ source: AsyncThrowingStream<Data, Error>, _ parse: @escaping @Sendable (Data) -> T?) -> AsyncThrowingStream<T, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    for try await data in source {
                        if let event = parse(data) { continuation.yield(event) }
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
        struct ScreenWrap: Decodable { var screen: ScreenState }
        struct RequestsWrap: Decodable { var requests: [AccessRequest] }
        struct CloudWrap: Decodable { var cloud: CloudStatus }
        let d = decoder
        guard let env = try? d.decode(Envelope.self, from: data) else { return nil }
        switch env.type {
        case "hello":
            let usage = (try? d.decode(UsageWrap.self, from: data))?.usage ?? Usage()
            let screen = (try? d.decode(ScreenWrap.self, from: data))?.screen
            return .hello(hostId: env.hostId ?? "", rev: env.rev ?? 0, usage: usage, screen: screen)
        case "bot":
            if let stub = try? d.decode(StubWrap.self, from: data), stub.bot.deleted == true {
                return .botDeleted(id: stub.bot.id, rev: stub.bot.rev)
            }
            return (try? d.decode(BotWrap.self, from: data)).map { .bot($0.bot) } ?? .undecodable(type: "bot")
        case "entry":
            return (try? d.decode(EntryWrap.self, from: data)).map { .entry($0.entry) } ?? .undecodable(type: "entry")
        case "usage":
            return (try? d.decode(UsageWrap.self, from: data)).map { .usage($0.usage) }
        case "screen":
            return (try? d.decode(ScreenWrap.self, from: data)).map { .screen($0.screen) }
        case "accessRequests":
            return (try? d.decode(RequestsWrap.self, from: data)).map { .accessRequests($0.requests) }
        case "cloud":
            return (try? d.decode(CloudWrap.self, from: data)).map { .cloud($0.cloud) }
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

    /// A thread's replies (the root is in the main chat).
    func thread(botId: String, rootId: String) async throws -> [Entry] {
        struct Res: Decodable { var entries: [Entry] }
        let res: Res = try await call("thread", ["botId": botId, "rootId": rootId])
        return res.entries
    }

    /// Creates a group chat (or returns the one these bots already share).
    func createGroup(_ draft: GroupDraft) async throws -> Bot {
        struct Res: Decodable { var bot: Bot }
        let res: Res = try await call("createBot", draft)
        return res.bot
    }

    func updateGroup(_ draft: GroupDraft) async throws -> Bot {
        struct Res: Decodable { var bot: Bot }
        let res: Res = try await call("updateBot", draft)
        return res.bot
    }

    func createBot(_ draft: BotDraft) async throws -> Bot {
        struct Res: Decodable { var bot: Bot }
        let res: Res = try await call("createBot", draft)
        return res.bot
    }

    func updateBot(_ draft: BotDraft) async throws -> Bot {
        struct Res: Decodable { var bot: Bot }
        let res: Res = try await call("updateBot", BotUpdate(draft: draft))
        return res.bot
    }

    func deleteBot(_ id: String) async throws {
        let _: Empty = try await call("deleteBot", ["botId": id])
    }

    /// The main chat, one thread (`threadId`), or everything (`all`).
    func markRead(_ id: String, threadId: String? = nil, all: Bool = false) async throws {
        struct Body: Encodable { var botId: String; var threadId: String?; var all: Bool }
        let _: Empty = try await call("markRead", Body(botId: id, threadId: threadId, all: all))
    }

    /// Toggles the user's `emoji` reaction on a chat message.
    func react(entryId: String, emoji: String) async throws -> Entry {
        struct Res: Decodable { var entry: Entry }
        let res: Res = try await call("react", ["entryId": entryId, "emoji": emoji])
        return res.entry
    }

    /// `threadId`: reply in the thread on that main-chat message.
    func send(botId: String, text: String, clientNonce: String, threadId: String? = nil, attachments: [String]? = nil) async throws -> Entry {
        struct Body: Encodable { var botId: String; var text: String; var clientNonce: String; var threadId: String?; var attachments: [String]? }
        struct Res: Decodable { var entry: Entry }
        let res: Res = try await call("send", Body(botId: botId, text: text, clientNonce: clientNonce, threadId: threadId, attachments: attachments))
        return res.entry
    }

    /// Bytes per `upload` call: base64 of it stays well under the channel's 1 MiB message.
    static let uploadChunk = 384 * 1024

    /// Sends one file in chunks for a later `send`; `id` names the upload there.
    func upload(botId: String, id: String, name: String, data: Data) async throws {
        struct Body: Encodable { var botId, uploadId, name, data: String; var offset: Int; var done: Bool }
        var offset = 0
        repeat {
            let end = min(offset + Self.uploadChunk, data.count)
            let chunk = data.subdata(in: offset..<end).base64EncodedString()
            let _: Empty = try await call("upload", Body(botId: botId, uploadId: id, name: name, data: chunk, offset: offset, done: end == data.count), timeout: 60)
            offset = end
        } while offset < data.count
    }

    /// A sent file, fetched back in chunks.
    func readUpload(botId: String, id: String) async throws -> Data {
        struct Body: Encodable { var botId, uploadId: String; var offset: Int }
        struct Res: Decodable { var data: String; var size: Int }
        var out = Data()
        repeat {
            let res: Res = try await call("readUpload", Body(botId: botId, uploadId: id, offset: out.count), timeout: 60)
            guard let chunk = Data(base64Encoded: res.data), !chunk.isEmpty else { break }
            out.append(chunk)
            if out.count >= res.size { break }
        } while true
        return out
    }

    func logCall(_ botId: String, seconds: Int) async throws {
        struct Body: Encodable { var botId: String; var seconds: Int }
        let _: Empty = try await call("logCall", Body(botId: botId, seconds: seconds))
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

    /// Push tickets are bound to this device's key; `pushKey` lets the host seal notification text (§6.7).
    func registerDevice(ticket: String, relay: String, name: String, pushKey: String?, ctx: String) async throws {
        struct Body: Encodable { var ticket: String; var relay: String; var name: String; var pushKey: String?; var ctx: String }
        let _: Empty = try await call("registerDevice", Body(ticket: ticket, relay: relay, name: name, pushKey: pushKey, ctx: ctx))
    }

    /// Stops this device's alerts; used when notifications are switched off in the app.
    func unregisterDevice() async throws {
        let _: Empty = try await call("unregisterDevice")
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

    // MARK: remote screen

    func screenStatus() async throws -> ScreenState { try await call("screenStatus") }

    func screenPrepare() async throws -> ScreenConnection {
        let relay = if let channel = transport as? ChannelTransport { try await channel.screenRelayRequired() } else { false }
        return try await call("screenPrepare", ["relay": relay])
    }

    /// Sends a WebRTC offer (with all its ICE candidates); returns the session id and the answer.
    /// Passing `session` restarts ICE on an existing session.
    func screenOffer(sdp: String, session: String?, display: UInt32?) async throws -> ScreenAnswer {
        struct Body: Encodable { var sdp: String; var session: String?; var display: UInt32? }
        return try await call("screenOffer", Body(sdp: sdp, session: session, display: display), timeout: 30)
    }

    func screenClose(session: String) async throws {
        let _: Empty = try await call("screenClose", ["session": session])
    }

    /// Takes control from bots (they can still look) or hands it back.
    func screenTakeover(_ on: Bool) async throws -> ScreenState {
        try await call("screenTakeover", ["on": on])
    }

    /// Only accepted from the computer itself.
    func setScreenEnabled(_ on: Bool) async throws -> ScreenState {
        try await call("setScreenEnabled", ["enabled": on])
    }
}

// MARK: - Agent setup

public extension HostClient {
    func refreshBackends() async throws -> [Backend] {
        struct Res: Decodable { var backends: [Backend] }
        let res: Res = try await call("refreshBackends", timeout: 60)
        return res.backends
    }

    /// Starts (or rejoins) installing or signing in to `backend` in a terminal on the computer.
    /// `method`: a terminal sign-in method from `agentAuth`; nil runs Codync's own command.
    func agentSetup(backend: String, step: SetupStep, method: String?, cols: Int, rows: Int) async throws -> String {
        struct Body: Encodable { var backend: String; var step: SetupStep; var method: String?; var cols: Int; var rows: Int }
        struct Res: Decodable { var term: String }
        // The first sign-in through a registry build can download it.
        let res: Res = try await call("agentSetup", Body(backend: backend, step: step, method: method, cols: cols, rows: rows), timeout: 300)
        return res.term
    }

    /// Starts the agent to see whether it's signed in and how it can be (first run may download it).
    func agentModels(_ backend: String) async throws -> AgentModels {
        try await call("agentModels", ["backend": backend], timeout: 300)
    }

    func agentAuth(_ backend: String) async throws -> AgentAuth {
        try await call("agentAuth", ["backend": backend], timeout: 300)
    }

    /// Lets the agent sign itself in; returns once it's done (or gave up).
    func agentAuthenticate(_ backend: String, method: String) async throws -> AgentAuth {
        try await call("agentAuthenticate", ["backend": backend, "method": method], timeout: 660)
    }

    /// Saves keys for the agent on the computer (empty values remove them).
    func setAgentEnv(_ backend: String, vars: [String: String]) async throws -> AgentAuth {
        struct Body: Encodable { var backend: String; var vars: [String: String] }
        return try await call("setAgentEnv", Body(backend: backend, vars: vars), timeout: 300)
    }

    func termInput(_ term: String, _ bytes: Data) async throws {
        let _: Empty = try await call("termInput", ["term": term, "data": bytes.base64EncodedString()])
    }

    func termResize(_ term: String, cols: Int, rows: Int) async throws {
        struct Body: Encodable { var term: String; var cols: Int; var rows: Int }
        let _: Empty = try await call("termResize", Body(term: term, cols: cols, rows: rows))
    }

    func termClose(_ term: String) async throws {
        let _: Empty = try await call("termClose", ["term": term])
    }
}

public struct ScreenAnswer: Codable, Sendable {
    public var session: String
    public var sdp: String
}

// MARK: - Loopback-only: this computer's devices and account (Mac)

public extension HostClient {
    /// The host signs a claim so the cloud can make this computer part of `userId`'s account (§4.2 A).
    func claimSign(claimId: String, nonce: String, userId: String) async throws -> ClaimSignature {
        try await call("claimSign", ["claimId": claimId, "nonce": nonce, "userId": userId])
    }

    func accessRequests() async throws -> [AccessRequest] {
        struct Res: Decodable { var requests: [AccessRequest] }
        let res: Res = try await call("accessRequests")
        return res.requests
    }

    func decideAccessRequest(_ id: String, approve: Bool) async throws {
        struct Body: Encodable { var requestId: String; var approve: Bool }
        let _: Empty = try await call("decideAccessRequest", Body(requestId: id, approve: approve), timeout: 30)
    }

    func devices() async throws -> [AuthorizedDevice] {
        struct Res: Decodable { var devices: [AuthorizedDevice] }
        let res: Res = try await call("devices")
        return res.devices
    }

    func revokeDevice(_ key: String) async throws {
        let _: Empty = try await call("revokeDevice", ["key": key])
    }

    func cloudStatus() async throws -> CloudStatus { try await call("cloudStatus") }

    func setCloud(enabled: Bool) async throws -> CloudStatus {
        try await call("setCloud", ["enabled": enabled], timeout: 30)
    }

    func setCloud(enabled: Bool, url: URL?) async throws -> CloudStatus {
        struct Body: Encodable { var enabled: Bool; var url: String? }
        return try await call("setCloud", Body(enabled: enabled, url: url?.absoluteString), timeout: 30)
    }

    func setApproval(_ approval: AccountApproval) async throws -> CloudStatus {
        try await call("setApproval", ["approval": approval.rawValue])
    }

    func unclaim() async throws {
        let _: Empty = try await call("unclaim", timeout: 30)
    }
}
