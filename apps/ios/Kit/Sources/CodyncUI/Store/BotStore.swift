import CodyncKit
import Foundation
import Observation

/// Opens the screen viewer, optionally watching a bot that's using the computer.
public struct ScreenRequest: Identifiable, Hashable, Sendable {
    public var watching: String?
    public var id: String { watching ?? "computer" }
    public init(watching: String? = nil) { self.watching = watching }
}

/// Where a message from this phone is: on its way, waiting in the relay mailbox for an offline
/// computer, accepted by the computer, or not sent.
public enum SendProgress: Sendable {
    case sending, queued, delivered, failed, cancelled
}

/// One computer's mirror (bots, transcripts) for a client (iPhone app, Mac window), kept by one
/// events stream (catch-up since `rev`, then live) over the encrypted channel.
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

    public private(set) var computer: Computer
    public internal(set) var connection: Connection = .connecting
    /// The first catch-up has arrived: everything up to the host's rev when this app connected.
    public internal(set) var caughtUp = false
    var catchUpRev = Int64.max
    /// The `since` this connection's events stream was opened with; the `hello` that opens its
    /// catch-up reads it (after a mirror reset `rev` no longer says what was requested).
    var requestedSince: Int64 = 0
    /// Per bot, the lowest synced main-chat `seq` held when this connection's catch-up began
    /// (empty when it began from rev 0). See `isOutsideLoadedWindow`.
    var windowFloors: [String: Int64] = [:]
    static let dropGrace: Duration = .seconds(5)
    static let initialConnectionGrace: Duration = .seconds(1)
    var heldDrop: Connection?
    var dropTimer: Task<Void, Never>?
    /// How long an action waits for a reconnect before it gives up.
    static let actionPatience: Duration = .seconds(20)
    /// Actions waiting for the link to come back.
    var waiting = 0
    public internal(set) var hostRoute: HostRoute?
    public internal(set) var client: HostClient?
    public internal(set) var hello: Hello?
    /// The host's version and `minApp`, kept even when the rest of its `hello` can't be read.
    public internal(set) var hostVersion: HostVersion?
    public internal(set) var bots: [String: Bot] = [:]
    public internal(set) var entries: [String: [Entry]] = [:]
    public internal(set) var usage: Usage
    /// The computer's remote screen (`nil`: the host predates it).
    public internal(set) var screen: ScreenState?
    /// Installed on the computer, for the Plugins screen and bot settings.
    public internal(set) var installedConnectors: [InstalledConnector] = []
    public internal(set) var installedSkills: [InstalledSkill] = []
    /// Bots whose older history has been fully paged in.
    public internal(set) var historyComplete: Set<String> = []
    var composerDrafts: [String: String] = [:]
    /// A dragged order not sent yet (see `reorder`).
    var orderSave: Task<Void, Never>?
    public var routineDrafts: [String: String] = [:]
    public let fileDownloads = FileDownloads()
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
    /// Each time the channel becomes ready.
    public var onConnected: (@MainActor (BotStore) -> Void)?
    public var onBotUpdated: (@MainActor (Bot) -> Void)?
    public var onUsageChanged: (@MainActor (ComputerID, Usage) -> Void)?
    /// A message to the bot from this phone moved along (the Live Activity follows it).
    public var onSent: (@MainActor (Bot, SendProgress) -> Void)?
    /// A bot appeared, changed or went away.
    var onRosterChanged: (@MainActor () -> Void)?
    /// The computer's addresses or keys changed (merged from its `hello`); persist it.
    var onComputerChanged: (@MainActor (Computer) -> Void)?

    /// `ios` clients suppress pushes while connected; others don't.
    let clientKind: String
    public let storage: SharedStore.Context
    let makeTransport: @MainActor () async throws -> any HostTransport
    var transport: (any HostTransport)?
    var retired = false
    /// Files of messages not delivered yet (by client nonce), kept for a retry.
    @ObservationIgnored var outgoingFiles: [String: [OutgoingFile]] = [:]

    var rev: Int64 = 0
    var hostId: String?
    var streamTask: Task<Void, Never>?
    var eventsTask: Task<Void, Never>?
    var isActive = true
    struct VoiceCall {
        let botId: String
        let startRev: Int64
        let speak: @MainActor (String) -> Void
        /// A notice from the computer (approval needed), as opposed to the bot's reply.
        let announce: @MainActor (String) -> Void
        let end: @MainActor () -> Void
    }
    @ObservationIgnored var voiceCalls: [UUID: VoiceCall] = [:]
    var saveTask: Task<Void, Never>?
    var rewound = false
    /// Events stayed undecodable after the rewind.
    var unreadable = false
    /// This app's marketing version, compared with the host's `minApp`.
    private let appVersion: String
    /// While one side needs an update there's nothing to sync; `hello` is asked again this often.
    static let mismatchRecheck: Duration = .seconds(30)

    /// Permission cards whose answer is on its way, with the chosen option: the card shows
    /// a spinner on it and takes no second answer.
    public internal(set) var answering: [String: String] = [:]

    @ObservationIgnored var readingViews: [UUID: ReadingScope] = [:]

    var cacheStamp: String?

    /// Loads this context's `DeviceIdentity` itself.
    public convenience init(computer: Computer, clientKind: String, storage: SharedStore.Context) {
        self.init(computer: computer, clientKind: clientKind, storage: storage) {
            try await HostConnector.connect(computer, identity: try DeviceIdentity.load(context: storage))
        }
    }

    init(computer: Computer, clientKind: String, storage: SharedStore.Context,
         appVersion: String = AppVersion.current,
         transport: @escaping @MainActor () async throws -> any HostTransport) {
        self.appVersion = appVersion
        self.computer = computer
        self.clientKind = clientKind
        self.storage = storage
        makeTransport = transport
        usage = storage.usage[computer.id] ?? Usage()
        composerDrafts = storage.composerDrafts[computer.id] ?? [:]
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
        // Only the warning is delayed; relay delivery can queue during that grace.
        if case .computerOffline = heldDrop ?? connection { computer.boxKey != nil && transport is any RemoteTransport } else { false }
    }

    /// What headers show: a reconnect the grace period keeps quiet reads "Connecting…"
    /// as soon as an action is waiting on it.
    public var shownConnection: Connection { waiting > 0 && connection == .online ? .connecting : connection }

    public var hostName: String { hello?.name ?? computer.name }

    /// Which side must update before this app and the host can work together
    /// (docs/reference/compatibility.md). `nil` until the host has answered, and whenever
    /// a version can't be read: an unknown is never a reason to lock someone out.
    public var mismatch: VersionMismatch? {
        guard let hostVersion else { return nil }
        if let found = VersionMismatch.check(app: appVersion, hostVersion: hostVersion.version, minApp: hostVersion.minApp) {
            return found
        }
        // The host sends data this app can't read and is newer: this app is behind,
        // whatever the host's `minApp` says.
        if unreadable, AppVersion.isBelow(appVersion, hostVersion.version) {
            return .updateApp(minimum: hostVersion.version)
        }
        return nil
    }

    /// Roster order: pinned first, then where the user dragged them, then most recent activity.
    public var roster: [Bot] {
        bots.values.filter { !$0.hidden }.sorted {
            if $0.pinned != $1.pinned { return $0.pinned }
            if $0.position != $1.position { return $0.position < $1.position }
            return $0.lastAt > $1.lastAt
        }
    }

    /// Display name for a harness id, as the host reports it (falls back to the built-in list).
    public func backendName(_ id: String) -> String {
        hello?.backends.first { $0.id == id }?.name ?? BackendInfo.name(id)
    }

    public var hiddenBots: [Bot] { bots.values.filter(\.hidden).sorted { $0.name < $1.name } }

    /// A bot's or group's main chat (thread replies are left out).
    public func chat(_ botId: String) -> [Entry] { (entries[botId] ?? []).filter { $0.threadId == nil } }

    /// Everything in a chat, threads included (the full conversation).
    public func allEntries(_ botId: String) -> [Entry] { entries[botId] ?? [] }

    /// A catch-up full of thread replies can leave only a few main-chat entries, or none.
    public func canLoadOlder(_ botId: String) -> Bool {
        !historyComplete.contains(botId) && allEntries(botId).contains { $0.seq > 0 && $0.seq != Int64.max }
    }

    /// The replies in the thread on `root`, oldest first.
    public func replies(_ botId: String, root: String) -> [Entry] { (entries[botId] ?? []).filter { $0.threadId == root } }

    /// A group's bots that still exist.
    public func members(of group: Bot) -> [Bot] { group.members.compactMap { bots[$0] } }

    /// Display name of an entry's author in a group chat.
    public func authorName(_ id: String?) -> String { id.flatMap { bots[$0]?.name } ?? "A deleted bot" }

    func updateComputer(_ change: (inout Computer) -> Void) {
        var c = computer
        change(&c)
        guard c != computer else { return }
        computer = c
        onComputerChanged?(c)
    }
}

/// A file picked in the composer, sent with the next message.
public struct OutgoingFile: Identifiable, Sendable {
    public let id = UUID()
    public let name: String
    public let data: Data

    public init(name: String, data: Data) {
        self.name = name
        self.data = data
    }

    /// Anything larger is refused by the computer.
    public static let maxSize = 100 * 1024 * 1024
}
