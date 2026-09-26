import Foundation

/// `b64url(SHA-256(hostSignPub)[0..16])`: derived from the host's signing key, never assigned by the cloud.
public typealias ComputerID = String

/// A computer this device may talk to. Holds no secrets: only pinned public keys and addresses.
public struct Computer: Codable, Hashable, Sendable, Identifiable {
    public var id: ComputerID
    public var name: String
    /// Pinned host signing key: the QR's `sk`, or the account flow's key once its SAS matched.
    public var signKey: String
    /// Pinned mailbox key: only from the QR's `bk` or the E2E `hello`; nil = no mailbox yet.
    public var boxKey: String?
    /// Direct candidates (`http://ip:port`), tried before the relay.
    public var urls: [String]
    /// The cloud relay base URL; nil = the host has the cloud off.
    public var cloud: URL?
    /// laptop · macmini · … · linux, from `hello`; picks the icon.
    public var device: String?
    /// The badge color picked on this device (an `AvatarPalette` id).
    public var color: String?

    public init(id: ComputerID, name: String, signKey: String, boxKey: String? = nil, urls: [String] = [],
                cloud: URL? = nil, device: String? = nil, color: String? = nil) {
        self.id = id
        self.name = name
        self.signKey = signKey
        self.boxKey = boxKey
        self.urls = urls
        self.cloud = cloud
        self.device = device
        self.color = color
    }

    /// `true` when `id` really is the computer ID of `signKey`.
    public var isConsistent: Bool {
        Data(base64URL: signKey).map { $0.count == 32 && RelayCrypto.computerId(signKey: $0) == id } ?? false
    }
}

/// A bot on a computer in an account context: bot IDs are only unique per computer.
/// (Multiple computers are future work; with one computer this is just the bot's address.)
public struct BotReference: Codable, Hashable, Sendable {
    /// `SharedStore.Context.accountID` (raw Clerk user ID; nil = the local context).
    public var accountId: String?
    public var computerId: ComputerID
    public var botId: String

    public init(accountId: String?, computerId: ComputerID, botId: String) {
        self.accountId = accountId
        self.computerId = computerId
        self.botId = botId
    }
}

/// One bot as the widgets see it.
public struct BotSnapshot: Codable, Hashable, Sendable {
    public var computerId: ComputerID
    public var bot: Bot

    public init(computerId: ComputerID, bot: Bot) {
        self.computerId = computerId
        self.bot = bot
    }
}
