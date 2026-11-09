import Foundation

private extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
}

// The messaging channels of a computer and its linked WhatsApp number. The calls and their
// answers live in the host (`telegramStatus`, `slackStatus`, `whatsappState`, …); these methods
// only carry them.

public extension HostClient {
    struct TelegramBody: Encodable {
        var name: String
        var userId: Int64?
        var allowed: Bool?
        var token: String?
        var botId: String?
    }

    /// The computer's Telegram bots. A host older than them answers without `bots`.
    func telegramBots() async throws -> [TelegramBot] {
        let status: TelegramStatus = try await call("telegramStatus", Empty())
        return status.bots
    }

    /// A one-time code to send to the Telegram bot `name`; nil from an older host.
    func pairTelegram(name: String) async throws -> String? {
        let status: TelegramStatus = try await call("pairTelegram", TelegramBody(name: name))
        return status.result?.code
    }

    /// Lets a Telegram user write to the bot `name`, or takes them out; the bots as they are then.
    func setTelegramAllowed(name: String, userId: Int64, allowed: Bool) async throws -> [TelegramBot] {
        let status: TelegramStatus = try await call("setTelegramAllowed",
                                                    TelegramBody(name: name, userId: userId, allowed: allowed))
        return status.bots
    }

    func slackStatus() async throws -> SlackStatus { try await call("slackStatus", Empty()) }

    /// Forgets the Slack connection, workspace and bot token included.
    func disconnectSlack() async throws -> SlackStatus { try await call("disconnectSlack", Empty()) }

    func discordStatus() async throws -> DiscordStatus { try await call("discordStatus", Empty()) }

    func disconnectDiscord() async throws -> DiscordStatus { try await call("disconnectDiscord", Empty()) }

    /// Sets the Discord bot and its bound channels up; the host checks the token before saving anything.
    func setDiscord(guildId: String, botUserId: String, botToken: String,
                    bindings: [DiscordBinding]) async throws -> DiscordStatus {
        struct Body: Encodable {
            var guildId: String
            var botUserId: String
            var botToken: String
            var bindings: [Binding]
        }
        struct Binding: Encodable {
            var channelId: String
            var botId: String
            var allowedUsers: [String]
        }
        let body = Body(guildId: guildId.trimmed, botUserId: botUserId.trimmed, botToken: botToken.trimmed,
                        bindings: bindings.map { Binding(channelId: $0.channelId.trimmed, botId: $0.botId,
                                                        allowedUsers: $0.allowedUsers.map(\.trimmed)) })
        return try await call("setDiscordChannel", body)
    }

    /// Adds a Telegram bot the host will talk through, answered by `botId`; its token stays on the computer.
    func addTelegramBot(name: String, token: String, botId: String) async throws -> [TelegramBot] {
        let status: TelegramStatus = try await call("addTelegramBot",
                                                    TelegramBody(name: name.trimmed, token: token.trimmed, botId: botId))
        return status.bots
    }

    /// Forgets a Telegram bot and its token.
    func removeTelegramBot(name: String) async throws -> [TelegramBot] {
        let status: TelegramStatus = try await call("removeTelegramBot", TelegramBody(name: name))
        return status.bots
    }

    func githubStatus() async throws -> GitHubStatus { try await call("githubStatus", Empty()) }

    func disconnectGitHub() async throws -> GitHubStatus { try await call("disconnectGitHub", Empty()) }

    /// Gives the host a GitHub token; it answers with the account that token belongs to.
    func setGitHubToken(_ token: String) async throws -> GitHubStatus {
        struct Body: Encodable { var token: String }
        return try await call("setGitHubToken", Body(token: token.trimmed))
    }

    func whatsappState() async throws -> WhatsAppLink { try await call("whatsappState", Empty()) }

    /// Starts the link; the host reports the QR code through `whatsappState`.
    func whatsappConnect() async throws {
        let _: [String: Bool] = try await call("whatsappConnect", Empty())
    }

    /// Links `phone` with a pairing code to type in WhatsApp instead of scanning a QR code.
    func whatsappPair(phone: String) async throws -> WhatsAppLink {
        struct Body: Encodable { var phone: String }
        return try await call("whatsappPair", Body(phone: phone))
    }

    /// Forgets the number on this computer.
    func whatsappLogout() async throws {
        let _: [String: Bool] = try await call("whatsappLogout", Empty())
    }

    /// Sends the number's messages to `botId`, or to none when nil.
    func whatsappSetBot(botId: String?) async throws {
        struct Body: Encodable {
            var botId: String?
            // `JSONEncoder` leaves a nil optional out; the host wants the key, empty for no bot.
            private enum CodingKeys: String, CodingKey { case botId }
            func encode(to encoder: Encoder) throws {
                var c = encoder.container(keyedBy: CodingKeys.self)
                try c.encode(botId ?? "", forKey: .botId)
            }
        }
        let _: [String: Bool] = try await call("whatsappSetBot", Body(botId: botId))
    }

    /// Lets a chat or person write, or takes them out. Returns the list as it is then.
    func whatsappSetAllowed(jid: String, allowed: Bool) async throws -> [String] {
        struct Body: Encodable { var jid: String; var allowed: Bool }
        struct Answer: Decodable { var allowed: [String] }
        let answer: Answer = try await call("whatsappSetAllowed", Body(jid: jid, allowed: allowed))
        return answer.allowed
    }
}
