import Foundation

// The messaging channels of a computer and its linked WhatsApp number. The calls and their
// answers live in the host (`telegramStatus`, `slackStatus`, `whatsappState`, …); these methods
// only carry them.

public extension HostClient {
    struct TelegramBody: Encodable {
        var name: String
        var userId: Int64?
        var allowed: Bool?
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

    func discordStatus() async throws -> DiscordStatus { try await call("discordStatus", Empty()) }

    func disconnectDiscord() async throws -> DiscordStatus { try await call("disconnectDiscord", Empty()) }

    func githubStatus() async throws -> GitHubStatus { try await call("githubStatus", Empty()) }

    func disconnectGitHub() async throws -> GitHubStatus { try await call("disconnectGitHub", Empty()) }

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
