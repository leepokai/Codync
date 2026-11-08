import CodyncKit
import Foundation

/// The computer's messaging channels: its Telegram bots, Slack connection, Discord server and
/// GitHub sign-in, and its linked WhatsApp number (host/src/telegram, slack, discord, github,
/// whatsapp.rs). The computer holds every token.
extension BotStore {
    public func telegramBots() async throws -> [TelegramBot] {
        try await ready().telegramBots()
    }

    /// A one-time pairing code for the Telegram bot `name`, and the bots as they are then.
    public func pairTelegram(name: String) async throws -> (code: String?, bots: [TelegramBot]) {
        let client = try await ready()
        let code = try await client.pairTelegram(name: name)
        return (code, (try? await client.telegramBots()) ?? [])
    }

    /// Lets a Telegram user write to the bot `name`, or takes them out.
    public func setTelegramAllowed(name: String, userId: Int64, allowed: Bool) async throws -> [TelegramBot] {
        try await ready().setTelegramAllowed(name: name, userId: userId, allowed: allowed)
    }

    public func slackStatus() async throws -> SlackStatus { try await ready().slackStatus() }

    public func discordStatus() async throws -> DiscordStatus { try await ready().discordStatus() }

    public func disconnectDiscord() async throws -> DiscordStatus { try await ready().disconnectDiscord() }

    public func githubStatus() async throws -> GitHubStatus { try await ready().githubStatus() }

    public func disconnectGitHub() async throws -> GitHubStatus { try await ready().disconnectGitHub() }

    /// The computer's linked WhatsApp number.
    public func whatsapp() async throws -> WhatsAppLink { try await ready().whatsappState() }

    /// Starts the link; the QR code arrives through `whatsapp()`.
    public func whatsappConnect() async throws { try await ready().whatsappConnect() }

    /// Links `phone` with a pairing code to type in WhatsApp instead of scanning a QR code.
    public func whatsappPair(phone: String) async throws -> WhatsAppLink {
        try await ready().whatsappPair(phone: phone)
    }

    /// Forgets the number on this computer.
    public func whatsappLogout() async throws { try await ready().whatsappLogout() }

    /// Sends the number's messages to `botId`, or to none when nil.
    public func whatsappSetBot(_ botId: String?) async throws { try await ready().whatsappSetBot(botId: botId) }

    /// Lets a chat or person write, or takes them out. Returns the list as it is then.
    public func whatsappSetAllowed(_ jid: String, allowed: Bool) async throws -> [String] {
        try await ready().whatsappSetAllowed(jid: jid, allowed: allowed)
    }
}
