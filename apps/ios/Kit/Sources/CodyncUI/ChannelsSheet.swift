import CodyncKit
import SwiftUI

/// The messaging channels of the computer (Telegram bots, Discord, GitHub and Slack): each bot's
/// state, when it last heard from someone, who may write to it and which bot answers. A phone can
/// pair someone and take people out; adding a Telegram bot or removing one needs its token, which
/// stays on the computer.
public struct ChannelsSheet: View {
    @Environment(BotStore.self) private var store
    @State private var bots: [TelegramBot]?
    @State private var slack: SlackStatus?
    @State private var discord: DiscordStatus?
    @State private var github: GitHubStatus?
    @State private var pairing: Pairing?
    @State private var error: String?
    @State private var busy = false

    private struct Pairing: Equatable {
        let bot: String
        let code: String
    }

    public init() {}

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Channels")
            CardForm {
                telegram
                discordSection
                githubSection
                slackSection
                if let error {
                    Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger)
                }
            }
        }
        .background(Palette.background)
        .task { await load() }
    }

    private var telegram: some View {
        CardSection("Telegram", footer: "Add a Telegram bot, or remove one, on the computer: its token stays there.") {
            if let list = bots {
                if list.isEmpty {
                    Text(L10n.tr("No Telegram bots yet.")).foregroundStyle(Palette.secondary)
                } else {
                    ForEach(list) { bot in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(handle(of: bot)).foregroundStyle(Palette.text)
                            Text(stateLine(of: bot)).font(.caption).foregroundStyle(Palette.secondary)
                            ForEach(bot.allowed, id: \.self) { userId in
                                ValueRow("Telegram user \(userId)") {
                                    Button("Remove") {
                                        act {
                                            bots = try await store.setTelegramAllowed(name: bot.name, userId: userId, allowed: false)
                                        }
                                    }
                                    .buttonStyle(.secondary)
                                    .disabled(busy)
                                }
                            }
                            if let pairing, pairing.bot == bot.name {
                                Text(L10n.tr(Channels.pairingInstruction(code: pairing.code, username: bot.username)))
                                    .font(.footnote)
                                    .foregroundStyle(Palette.text)
                            }
                            Button("Pair someone") {
                                act {
                                    let answer = try await store.pairTelegram(name: bot.name)
                                    if let code = answer.code { pairing = Pairing(bot: bot.name, code: code) }
                                    bots = answer.bots
                                }
                            }
                            .buttonStyle(.secondary)
                            .disabled(busy)
                        }
                        .padding(.vertical, 2)
                    }
                }
            } else {
                Text(L10n.tr("Loading…")).foregroundStyle(Palette.secondary)
            }
        }
    }

    private var discordSection: some View {
        CardSection("Discord", footer: "Set up Discord on the computer: its bot token stays there. Only the people allowed in each channel can write to it.") {
            VStack(alignment: .leading, spacing: 6) {
                Text(L10n.tr(discordLine)).foregroundStyle(Palette.secondary)
                ForEach(discord?.bindings ?? []) { binding in
                    Text("\(binding.channelId) · \(binding.allowedUsers.count) \(L10n.tr("people allowed"))")
                        .font(.caption)
                        .foregroundStyle(Palette.tertiary)
                }
                if discord?.configured == true {
                    Button("Disconnect Discord") {
                        act {
                            discord = try await store.disconnectDiscord()
                            discord = try await store.discordStatus()
                        }
                    }
                    .buttonStyle(.secondary)
                    .disabled(busy)
                }
            }
            .padding(.vertical, 2)
        }
    }

    private var githubSection: some View {
        CardSection("GitHub", footer: "Set up GitHub on the computer: the token stays there. Workflows can comment, label and open issues with it.") {
            VStack(alignment: .leading, spacing: 6) {
                Text(L10n.tr(githubLine)).foregroundStyle(Palette.secondary)
                if let github, github.configured {
                    if !github.scopes.isEmpty {
                        Text("\(L10n.tr("Scopes")): \(github.scopes.joined(separator: ", "))")
                            .font(.caption)
                            .foregroundStyle(Palette.tertiary)
                    }
                    Button("Disconnect GitHub") {
                        act { github = try await store.disconnectGitHub() }
                    }
                    .buttonStyle(.secondary)
                    .disabled(busy)
                }
            }
            .padding(.vertical, 2)
        }
    }

    private var slackSection: some View {
        CardSection("Slack") {
            Text(L10n.tr(slackLine))
                .foregroundStyle(Palette.secondary)
                .padding(.vertical, 2)
        }
    }

    private var discordLine: String {
        guard let discord else { return "Loading…" }
        guard discord.configured else { return "Discord is not connected." }
        return "\(L10n.tr(Channels.discordStateText(discord.state))) · \(discord.bindings.count) \(L10n.tr("channels bound")) · \(L10n.tr("Server")) \(discord.guildId ?? "")"
    }

    private var githubLine: String {
        guard let github else { return "Loading…" }
        guard github.configured else { return "GitHub is not connected." }
        return "\(L10n.tr("Signed in as")) \(github.login ?? "")"
    }

    private var slackLine: String {
        guard let slack else { return "Loading…" }
        guard slack.configured else { return "Slack is not connected." }
        return slack.socketMode
            ? "Connected through Socket Mode · \(slack.workspaceId ?? "")"
            : "Connected through the HTTP endpoint · \(slack.workspaceId ?? "")"
    }

    private func handle(of bot: TelegramBot) -> String {
        let at = bot.username.isEmpty ? "" : " · @\(bot.username)"
        return "\(bot.name)\(at) → \(bot.botName)"
    }

    private func stateLine(of bot: TelegramBot) -> String {
        "\(L10n.tr(Channels.stateText(bot.state))) · \(L10n.tr(Channels.lastReceivedText(at: bot.lastReceivedAt))) · \(L10n.tr(Channels.allowedText(bot.allowed.count)))"
    }

    private func load() async {
        do {
            bots = try await store.telegramBots()
        } catch {
            bots = []
            self.error = error.localizedDescription
        }
        slack = try? await store.slackStatus()
        discord = try? await store.discordStatus()
        github = try? await store.githubStatus()
    }

    /// One change at a time; the computer's answer replaces what is shown.
    private func act(_ work: @escaping @MainActor () async throws -> Void) {
        guard !busy else { return }
        busy = true
        error = nil
        Task {
            do {
                try await work()
            } catch {
                self.error = error.localizedDescription
            }
            busy = false
        }
    }
}
