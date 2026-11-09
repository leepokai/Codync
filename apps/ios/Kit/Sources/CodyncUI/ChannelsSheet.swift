import CodyncKit
import SwiftUI

/// The messaging channels of the computer (Telegram bots, Discord, GitHub and Slack): each bot's
/// state, when it last heard from someone, who may write to it and which bot answers. The computer
/// holds every token; a phone sets the channels up, pairs people to a bot and takes them out,
/// exactly as the computer's own settings do.
public struct ChannelsSheet: View {
    @Environment(BotStore.self) private var store
    @State private var bots: [TelegramBot]?
    @State private var slack: SlackStatus?
    @State private var discord: DiscordStatus?
    @State private var github: GitHubStatus?
    @State private var pairing: Pairing?
    @State private var error: String?
    @State private var busy = false
    @State private var removing: String?
    @State private var adding = false
    @State private var newName = ""
    @State private var newToken = ""
    @State private var newBot = ""
    @State private var editingDiscord = false
    @State private var guildId = ""
    @State private var botUserId = ""
    @State private var discordToken = ""
    @State private var rows: [BindingDraft] = []
    @State private var githubToken = ""

    private struct Pairing: Equatable {
        let bot: String
        let code: String
    }

    /// One channel the Discord form is filling in.
    private struct BindingDraft: Identifiable, Equatable {
        let id = UUID()
        var channelId: String
        var botId: String
        var allowed: String
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
        .codyncDialog("Remove this Telegram bot?", isPresented: Binding(
            get: { removing != nil }, set: { if !$0 { removing = nil } }
        ), message: "Its token is deleted from the computer and the people paired to it can no longer write to your bot.") {
            [DialogAction("Remove", destructive: true) {
                guard let name = removing else { return }
                removing = nil
                act { bots = try await store.removeTelegramBot(name: name) }
            }]
        }
    }

    // The bots a channel may answer with: not a group, not a terminal session.
    private var choices: [Bot] {
        store.bots.values.filter { !$0.isGroup && $0.kind != "external" }.sorted { $0.name < $1.name }
    }

    private var fallbackBot: String { choices.first?.id ?? "" }

    private var telegram: some View {
        CardSection("Telegram", footer: "Add a Telegram bot, or remove one, here or on the computer: its token stays there.") {
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
                            Button("Remove bot") {
                                removing = bot.name
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
            if adding {
                VStack(alignment: .leading, spacing: 8) {
                    Text(L10n.tr("Telegram bot name")).font(.caption).foregroundStyle(Palette.secondary)
                    plainField($newName, prompt: "A short name: home, work…")
                    Text(L10n.tr("Telegram bot token")).font(.caption).foregroundStyle(Palette.secondary)
                    plainField($newToken, prompt: "The token @BotFather gave you")
                    ChoicePicker(selection: Binding(
                        get: { newBot.isEmpty ? fallbackBot : newBot },
                        set: { newBot = $0 }
                    ), options: choices.map { (id: $0.id, label: $0.name) })
                    Button(busy ? L10n.tr("Adding…") : L10n.tr("Add Telegram bot")) {
                        let botId = newBot.isEmpty ? fallbackBot : newBot
                        act {
                            bots = try await store.addTelegramBot(name: newName, token: newToken, botId: botId)
                            newName = ""
                            newToken = ""
                            newBot = ""
                            adding = false
                        }
                    }
                    .buttonStyle(.secondary)
                    .disabled(busy || !Channels.nameOk(newName.trimmed) || !Channels.tokenOk(newToken.trimmed) || choices.isEmpty)
                }
                .padding(.vertical, 2)
            } else {
                Button("Add a bot") { adding = true }
                    .buttonStyle(.secondary)
                    .disabled(busy || bots == nil)
            }
        }
    }

    private var discordSection: some View {
        CardSection("Discord", footer: "Make a Discord app, add its bot to the server, and paste the bot token here. In the app, turn on the Message Content intent. Only the people allowed in each channel can write to it.") {
            VStack(alignment: .leading, spacing: 6) {
                if let discord, discord.configured, !editingDiscord {
                    Text(L10n.tr(discordLine(discord))).foregroundStyle(Palette.secondary)
                    ForEach(discord.bindings) { binding in
                        Text("\(binding.channelId) · \(binding.allowedUsers.count) \(L10n.tr("people allowed"))")
                            .font(.caption)
                            .foregroundStyle(Palette.tertiary)
                    }
                    Button("Change Discord settings") { startDiscord() }
                        .buttonStyle(.secondary)
                        .disabled(busy)
                    Button("Disconnect Discord") {
                        act {
                            discord = try await store.disconnectDiscord()
                            discord = try await store.discordStatus()
                        }
                    }
                    .buttonStyle(.secondary)
                    .disabled(busy)
                } else if discord == nil, !editingDiscord {
                    Text(L10n.tr("Loading…")).foregroundStyle(Palette.secondary)
                } else {
                    if editingDiscord && rows.isEmpty { startDiscord() }
                    Text(L10n.tr("Server id")).font(.caption).foregroundStyle(Palette.secondary)
                    plainField($guildId, prompt: "123456789012345678")
                    Text(L10n.tr("Bot user id")).font(.caption).foregroundStyle(Palette.secondary)
                    plainField($botUserId, prompt: "123456789012345678")
                    Text(L10n.tr("Bot token")).font(.caption).foregroundStyle(Palette.secondary)
                    plainField($discordToken, prompt: "The token from the Bot tab")
                    ForEach($rows) { $row in
                        Text(L10n.tr("Channel id")).font(.caption).foregroundStyle(Palette.secondary)
                        plainField($row.channelId, prompt: "123456789012345678")
                        ChoicePicker(selection: Binding(
                            get: { row.botId.isEmpty ? fallbackBot : row.botId },
                            set: { row.botId = $0 }
                        ), options: choices.map { (id: $0.id, label: $0.name) })
                        Text(L10n.tr("Allowed user ids")).font(.caption).foregroundStyle(Palette.secondary)
                        plainField($row.allowed, prompt: "Separated by spaces or commas")
                        if rows.count > 1 {
                            Button("Remove channel") {
                                rows.removeAll { $0.id == row.id }
                            }
                            .buttonStyle(.secondary)
                        }
                    }
                    HStack(spacing: 8) {
                        Button("Add channel") { rows.append(BindingDraft(channelId: "", botId: "", allowed: "")) }
                            .buttonStyle(.secondary)
                        Button(busy ? L10n.tr("Checking…") : L10n.tr("Connect Discord")) {
                            let bindings = cleanBindings
                            act {
                                discord = try await store.setDiscord(guildId: guildId, botUserId: botUserId,
                                                                     botToken: discordToken, bindings: bindings)
                                editingDiscord = false
                                discordToken = ""
                            }
                        }
                        .buttonStyle(.secondary)
                        .disabled(busy || !discordReady)
                        if discord?.configured == true {
                            Button("Cancel") { editingDiscord = false }
                                .buttonStyle(.secondary)
                                .disabled(busy)
                        }
                    }
                }
            }
            .padding(.vertical, 2)
        }
    }

    private var githubSection: some View {
        CardSection("GitHub", footer: "A fine-grained token with the repository permissions your workflows need. It stays on the computer; Codync never sends it anywhere else.") {
            VStack(alignment: .leading, spacing: 6) {
                if let github, github.configured {
                    Text("\(L10n.tr("Signed in as")) \(github.login ?? "")").foregroundStyle(Palette.secondary)
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
                } else if github != nil {
                    plainField($githubToken, prompt: "github_pat_…")
                    Button(busy ? L10n.tr("Checking…") : L10n.tr("Connect GitHub")) {
                        act {
                            github = try await store.setGitHubToken(githubToken)
                            githubToken = ""
                        }
                    }
                    .buttonStyle(.secondary)
                    .disabled(busy || !Channels.githubTokenOk(githubToken))
                } else {
                    Text(L10n.tr("Loading…")).foregroundStyle(Palette.secondary)
                }
            }
            .padding(.vertical, 2)
        }
    }

    private var slackSection: some View {
        CardSection("Slack", footer: "Slack is set up with the host's setSlackChannel call; see the Slack checklist.") {
            VStack(alignment: .leading, spacing: 6) {
                if let slack {
                    Text(slackLine(slack)).foregroundStyle(Palette.secondary)
                    if slack.configured {
                        Button("Disconnect Slack") {
                            act { slack = try await store.disconnectSlack() }
                        }
                        .buttonStyle(.secondary)
                        .disabled(busy)
                    }
                } else {
                    Text(L10n.tr("Loading…")).foregroundStyle(Palette.secondary)
                }
            }
            .padding(.vertical, 2)
        }
    }

    private func discordLine(_ discord: DiscordStatus) -> String {
        "\(L10n.tr(Channels.discordStateText(discord.state))) · \(discord.bindings.count) \(L10n.tr("channels bound")) · \(L10n.tr("Server")) \(discord.guildId ?? "")"
    }

    private func slackLine(_ slack: SlackStatus) -> String {
        guard slack.configured else { return L10n.tr("Slack is not connected.") }
        return slack.socketMode
            ? "\(L10n.tr("Connected through Socket Mode")) · \(slack.workspaceId ?? "")"
            : "\(L10n.tr("Connected through the HTTP endpoint")) · \(slack.workspaceId ?? "")"
    }

    private func handle(of bot: TelegramBot) -> String {
        let at = bot.username.isEmpty ? "" : " · @\(bot.username)"
        return "\(bot.name)\(at) → \(bot.botName)"
    }

    private func stateLine(of bot: TelegramBot) -> String {
        "\(L10n.tr(Channels.stateText(bot.state))) · \(L10n.tr(Channels.lastReceivedText(at: bot.lastReceivedAt))) · \(L10n.tr(Channels.allowedText(bot.allowed.count)))"
    }

    /// A token or an id: no chrome, the placeholder in the secondary ink.
    private func plainField(_ text: Binding<String>, prompt: String) -> some View {
        TextField(prompt, text: text)
            .textFieldStyle(.plain)
            .plainTextInput()
            .autocorrectionDisabled()
            .textInputAutocapitalization(.never)
            .padding(.horizontal, 14)
            .frame(height: 38)
            .background(Palette.bubbleUser, in: Capsule())
    }

    /// The rows with a channel id, as the host wants them.
    private var cleanBindings: [DiscordBinding] {
        rows.compactMap { row in
            let channel = row.channelId.trimmed
            let allowed = row.allowed.split(whereSeparator: { $0.isWhitespace || $0 == "," }).map(String.init)
            guard !channel.isEmpty || !allowed.isEmpty else { return nil }
            return DiscordBinding(channelId: channel, botId: row.botId.isEmpty ? fallbackBot : row.botId,
                                  allowedUsers: allowed)
        }
    }

    private var discordReady: Bool {
        Channels.idOk(guildId.trimmed) && Channels.idOk(botUserId.trimmed)
            && Channels.discordTokenOk(discordToken.trimmed) && !cleanBindings.isEmpty
            && cleanBindings.allSatisfy {
                Channels.idOk($0.channelId) && !$0.botId.isEmpty && !$0.allowedUsers.isEmpty
            }
    }

    private func startDiscord() {
        guildId = discord?.guildId ?? ""
        botUserId = discord?.botUserId ?? ""
        discordToken = ""
        rows = (discord?.bindings ?? []).map {
            BindingDraft(channelId: $0.channelId, botId: $0.botId, allowed: $0.allowedUsers.joined(separator: ", "))
        }
        if rows.isEmpty { rows = [BindingDraft(channelId: "", botId: "", allowed: "")] }
        editingDiscord = true
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

private extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
}
