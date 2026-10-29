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
        /// Can't reach the computer at all (no direct route, no relay), or it can't confirm this
        /// device's access for now.
        case offline(String)
        /// Revoked, or the computer's identity changed.
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
    /// An update this phone asked of the computer; cleared when its version changes.
    public internal(set) var hostUpdate: HostUpdate?
    public internal(set) var bots: [String: Bot] = [:]
    public internal(set) var entries: [String: [Entry]] = [:]
    public internal(set) var usage: Usage
    /// The computer's remote screen (`nil`: the host predates it).
    public internal(set) var screen: ScreenState?
    /// The computer's network of the user's computers (`nil`: the host predates it).
    public internal(set) var network: NetworkView?
    /// Installed on the computer, for the Plugins screen and bot settings.
    public internal(set) var installedConnectors: [InstalledConnector] = []
    public internal(set) var installedSkills: [InstalledSkill] = []
    /// Bots whose older history has been fully paged in.
    public internal(set) var historyComplete: Set<String> = []
    var composerDrafts: [String: String] = [:]
    /// What was pasted into each draft, sent along so memory never takes it as the user's own words.
    @ObservationIgnored var composerPasted: [String: [String]] = [:]
    public var routineDrafts: [String: String] = [:]
    public let fileDownloads = FileDownloads()
    var artifacts = ArtifactCache()
    public var lastError: String?
    /// The open conversation (navigation path).
    public var selection: String?
    /// The remote screen viewer (iPhone); `codync://screen` opens it.
    public var screenRequest: ScreenRequest?

    /// A Shortcuts request, consumed by the matching conversation.
    public internal(set) var voiceCallRequest: String?
    @ObservationIgnored var preparingVoiceCall: UUID?

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

    public let storage: SharedStore.Context
    /// Opens a channel to the computer as it is now: its route, and the addresses and keys merged since.
    let makeTransport: @MainActor (Computer) async throws -> any HostTransport
    var transport: (any HostTransport)?
    var retired = false
    /// Files of messages not delivered yet (by client nonce), kept for a retry.
    @ObservationIgnored var outgoingFiles: [String: [OutgoingFile]] = [:]

    var rev: Int64 = 0
    var hostId: String?
    var streamTask: Task<Void, Never>?
    var eventsTask: Task<Void, Never>?
    var isActive = true
    /// A call's audio: what the computer tells its voice (`CallCue`), and its end.
    struct VoiceCall {
        let hear: @MainActor (CallCue) -> Void
        let end: @MainActor () -> Void
    }
    @ObservationIgnored var voiceCalls: [UUID: VoiceCall] = [:]
    /// The newest cue each call heard (`CallCue.seq`), kept across engine switches: a replayed one isn't said again.
    @ObservationIgnored var heardCues: [UUID: Int64] = [:]
    @ObservationIgnored var shortcutSends = 0
    /// Calls whose end is still on its way to the computer.
    @ObservationIgnored var callEnds = 0
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

    /// The chats on screen, by the view showing each.
    @ObservationIgnored var readingViews: [UUID: String] = [:]

    var cacheStamp: String?

    /// Loads this context's `DeviceIdentity` itself.
    public convenience init(computer: Computer, storage: SharedStore.Context) {
        self.init(computer: computer, storage: storage) { current in
            try await HostConnector.connect(current, identity: try DeviceIdentity.load(context: storage))
        }
    }

    init(computer: Computer, storage: SharedStore.Context,
         appVersion: String = AppVersion.current,
         transport: @escaping @MainActor (Computer) async throws -> any HostTransport) {
        self.appVersion = appVersion
        self.computer = computer
        self.storage = storage
        makeTransport = transport
        usage = storage.usage[computer.id] ?? Usage()
        composerDrafts = Self.chatDrafts(storage.composerDrafts[computer.id] ?? [:])
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

    /// Roster order: the lead, pinned bots, then where the user dragged them, then most recent activity.
    public var roster: [Bot] {
        bots.values.filter { !$0.hidden }.sorted {
            if $0.band != $1.band { return $0.band < $1.band }
            if $0.position != $1.position { return $0.position < $1.position }
            return $0.lastAt > $1.lastAt
        }
    }

    /// Display name for a harness id, as the host reports it (falls back to the built-in list).
    public func backendName(_ id: String) -> String {
        hello?.backends.first { $0.id == id }?.name ?? BackendInfo.name(id)
    }

    /// The ACP registry id an agent's logo is under.
    func agentRegistry(_ id: String) -> String {
        hello?.backends.first { $0.id == id }?.registry ?? id
    }

    /// An agent as the Agent pickers list it.
    func agentLabel(_ backend: Backend) -> String {
        backend.available ? backend.name : "\(backend.name) (not installed)"
    }

    public var hiddenBots: [Bot] { bots.values.filter(\.hidden).sorted { $0.name < $1.name } }

    /// Everything in a bot's or group's chat this device has, oldest first (trace included).
    public func chat(_ botId: String) -> [Entry] { entries[botId] ?? [] }

    public func canLoadOlder(_ botId: String) -> Bool {
        !historyComplete.contains(botId) && chat(botId).contains { $0.seq > 0 && $0.seq != Int64.max }
    }

    /// A group's bots that still exist.
    public func members(of group: Bot) -> [Bot] { group.members.compactMap { bots[$0] } }

    /// Display name of an entry's author; `fallback` for one that isn't this computer's (a bot on another computer).
    public func authorName(_ id: String?, fallback: String? = nil) -> String {
        id.flatMap { bots[$0]?.name } ?? fallback ?? "A deleted bot"
    }

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
