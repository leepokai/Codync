import Foundation

// The messaging channels of a computer (host/src/telegram, host/src/slack, host/src/discord,
// host/src/github) and its linked WhatsApp number (host/src/whatsapp.rs). The host owns the
// meaning and keeps the tokens; these shapes only carry what the screens show, and their words
// come from `Channels` and `WhatsApp`.

/// A Telegram bot of the computer, as `telegramStatus` lists it. Never carries the token.
public struct TelegramBot: Decodable, Identifiable, Equatable, Sendable {
    public var name: String
    public var username: String
    public var botId: String
    public var botName: String
    /// The Telegram user ids that may write to it.
    public var allowed: [Int64]
    /// `ok`, `starting`, or `error: …`.
    public var state: String
    public var lastReceivedAt: Int64?
    public var pairingExpiresAt: Int64?

    public var id: String { name }

    private enum CodingKeys: String, CodingKey {
        case name, username, botId, botName, allowed, state, lastReceivedAt, pairingExpiresAt
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        username = try c.decodeIfPresent(String.self, forKey: .username) ?? ""
        botId = try c.decodeIfPresent(String.self, forKey: .botId) ?? ""
        botName = try c.decodeIfPresent(String.self, forKey: .botName) ?? ""
        allowed = try c.decodeIfPresent([Int64].self, forKey: .allowed) ?? []
        state = try c.decodeIfPresent(String.self, forKey: .state) ?? "starting"
        lastReceivedAt = try c.decodeIfPresent(Int64.self, forKey: .lastReceivedAt)
        pairingExpiresAt = try c.decodeIfPresent(Int64.self, forKey: .pairingExpiresAt)
    }
}

/// The answer to a Telegram call: the bots as they are then, and what the call made.
public struct TelegramStatus: Decodable, Sendable {
    public var configured: Bool
    public var bots: [TelegramBot]
    /// The one-time code a `pairTelegram` answer carries.
    public var result: TelegramPairing?

    private enum CodingKeys: String, CodingKey { case configured, bots, result }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        configured = try c.decodeIfPresent(Bool.self, forKey: .configured) ?? false
        bots = try c.decodeIfPresent([TelegramBot].self, forKey: .bots) ?? []
        result = try c.decodeIfPresent(TelegramPairing.self, forKey: .result)
    }
}

public struct TelegramPairing: Decodable, Sendable {
    public var code: String
    public var expiresAt: Int64?

    private enum CodingKeys: String, CodingKey { case code, expiresAt }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        code = try c.decode(String.self, forKey: .code)
        expiresAt = try c.decodeIfPresent(Int64.self, forKey: .expiresAt)
    }
}

/// The Slack connection, as `slackStatus` reports it.
public struct SlackStatus: Decodable, Equatable, Sendable {
    public var configured: Bool
    public var socketMode: Bool
    public var workspaceId: String?

    private enum CodingKeys: String, CodingKey { case configured, socketMode, workspaceId }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        configured = try c.decodeIfPresent(Bool.self, forKey: .configured) ?? false
        socketMode = try c.decodeIfPresent(Bool.self, forKey: .socketMode) ?? false
        workspaceId = try c.decodeIfPresent(String.self, forKey: .workspaceId)
    }
}

/// One bound Discord channel: the host bot that answers it and the people allowed in it.
public struct DiscordBinding: Decodable, Identifiable, Equatable, Sendable {
    public var channelId: String
    public var botId: String
    public var allowedUsers: [String]

    public var id: String { channelId }

    private enum CodingKeys: String, CodingKey { case channelId, botId, allowedUsers }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        channelId = try c.decode(String.self, forKey: .channelId)
        botId = try c.decodeIfPresent(String.self, forKey: .botId) ?? ""
        allowedUsers = try c.decodeIfPresent([String].self, forKey: .allowedUsers) ?? []
    }
}

/// The Discord connection, as `discordStatus` reports it.
public struct DiscordStatus: Decodable, Equatable, Sendable {
    public var configured: Bool
    public var guildId: String?
    public var botUserId: String?
    public var bindings: [DiscordBinding]
    /// `ok`, `starting`, or `error: …`.
    public var state: String

    private enum CodingKeys: String, CodingKey { case configured, guildId, botUserId, bindings, state }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        configured = try c.decodeIfPresent(Bool.self, forKey: .configured) ?? false
        guildId = try c.decodeIfPresent(String.self, forKey: .guildId)
        botUserId = try c.decodeIfPresent(String.self, forKey: .botUserId)
        bindings = try c.decodeIfPresent([DiscordBinding].self, forKey: .bindings) ?? []
        state = try c.decodeIfPresent(String.self, forKey: .state) ?? "starting"
    }
}

/// The GitHub connection, as `githubStatus` reports it. Never carries the token.
public struct GitHubStatus: Decodable, Equatable, Sendable {
    public var configured: Bool
    public var login: String?
    public var scopes: [String]

    private enum CodingKeys: String, CodingKey { case configured, login, scopes }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        configured = try c.decodeIfPresent(Bool.self, forKey: .configured) ?? false
        login = try c.decodeIfPresent(String.self, forKey: .login)
        scopes = try c.decodeIfPresent([String].self, forKey: .scopes) ?? []
    }
}

/// The computer's linked WhatsApp number, as `whatsappState` reports it.
public struct WhatsAppLink: Decodable, Equatable, Sendable {
    /// `disconnected`, `starting`, `qr`, `pairing`, `connected`, `loggedOut` or `error`.
    public var state: String
    public var qr: String?
    /// The code to type in `WhatsApp` → Linked devices instead of scanning a QR code.
    public var pairCode: String?
    public var me: String?
    public var name: String?
    public var error: String?
    /// The bot whose chat receives this number's messages.
    public var bot: String?
    /// Chats and people allowed to write; empty means anyone.
    public var allowed: [String]

    private enum CodingKeys: String, CodingKey { case state, qr, pairCode, me, name, error, bot, allowed }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        state = try c.decodeIfPresent(String.self, forKey: .state) ?? "disconnected"
        qr = try c.decodeIfPresent(String.self, forKey: .qr)
        pairCode = try c.decodeIfPresent(String.self, forKey: .pairCode)
        me = try c.decodeIfPresent(String.self, forKey: .me)
        name = try c.decodeIfPresent(String.self, forKey: .name)
        error = try c.decodeIfPresent(String.self, forKey: .error)
        bot = try c.decodeIfPresent(String.self, forKey: .bot)
        allowed = try c.decodeIfPresent([String].self, forKey: .allowed) ?? []
    }
}

/// What the Channels screen shows, and the words it uses.
public enum Channels {
    public static func stateText(_ state: String) -> String {
        switch state {
        case "ok": "Connected"
        case "starting": "Starting…"
        default: "Cannot reach Telegram"
        }
    }

    public static func discordStateText(_ state: String) -> String {
        switch state {
        case "ok": "Connected"
        case "starting": "Connecting…"
        default: "Cannot reach Discord"
        }
    }

    public static func lastReceivedText(at: Int64?, now: Int64 = Int64(Date().timeIntervalSince1970 * 1000)) -> String {
        guard let at else { return "Nothing received yet" }
        let minutes = max(0, (now - at) / 60_000)
        return switch minutes {
        case ..<1: "Last message just now"
        case ..<60: "Last message \(minutes) min ago"
        case ..<(60 * 24): "Last message \(minutes / 60) h ago"
        default: "Last message \(minutes / (60 * 24)) d ago"
        }
    }

    /// Where to send the code, which works once for ten minutes.
    public static func pairingInstruction(code: String, username: String) -> String {
        let destination = username.isEmpty ? "your Telegram bot" : "@\(username)"
        return "Send /start \(code) to \(destination) in Telegram. The code works once, for 10 minutes."
    }

    public static func allowedText(_ count: Int) -> String {
        count == 1 ? "1 person allowed" : "\(count) people allowed"
    }
}

/// What the WhatsApp screen shows, and the words it uses.
public enum WhatsApp {
    public static func stateText(_ state: String) -> String {
        switch state {
        case "connected": "Linked"
        case "starting": "Starting…"
        case "qr": "Scan the code"
        case "pairing": "Enter the code on your phone."
        case "error": "The link stopped"
        default: "Not linked"
        }
    }

    /// The code in two groups of four, as WhatsApp shows its field for it.
    public static func groupedCode(_ code: String) -> String {
        code.count == 8 ? "\(code.prefix(4))-\(code.suffix(4))" : code
    }
}
