import CodyncKit
import SwiftUI

/// The page a thread starts from, pushed over the Threads list: the desktop's `ThreadStart`, a chat nothing was said
/// in yet. In the middle who it's with (the bot's avatar, or without a bot the agent's logo) and "New thread with …";
/// under it the chat's composer, its row holding the bot chip (the bots from `bots`, each on its own model, approvals
/// and folder, then **No bot**) and, without a bot, the model chip (the model panel). It opens on no bot on the
/// computer `plain` that starts one that way, else on the lead bot. Without a bot it runs on the agent and model the
/// panel picks, in a folder of its own (an older host: the computer's home folder), asking before anything it does, in
/// the agent's Auto mode. Text only: `startThread` takes no files. Sent, the page becomes the new thread's chat once it
/// has its thread, so Back goes to the list; with none within 10 seconds, the list, then the bot's details
/// (`openDetails`; a thread without a bot: nothing).
public struct NewThreadView: View {
    let bots: [RosterItem]
    /// The computer a thread without a bot starts on; nil where none can.
    let plain: BotStore?
    let store: (ComputerID) -> BotStore?
    let item: (_ computer: ComputerID, _ botId: String) -> RosterItem?
    let openDetails: (RosterItem) -> Void
    @Environment(\.dismiss) private var dismiss
    /// The bot it's with; nil for no bot.
    @State private var with: BotReference?
    @State private var login: AgentLogin
    @State private var model: String?
    @State private var options: [String: String] = [:]
    @State private var choosingModel = false
    @State private var error: String?
    /// The thread it started, shown in this page's place.
    @State private var started: ThreadTarget?

    /// One draft per computer for the new thread, whoever it's with.
    private static let draftKey = "thread/new"

    public init(bots: [RosterItem], plain: BotStore?, store: @escaping (ComputerID) -> BotStore?,
                item: @escaping (_ computer: ComputerID, _ botId: String) -> RosterItem?, openDetails: @escaping (RosterItem) -> Void) {
        self.bots = bots
        self.plain = plain
        self.store = store
        self.item = item
        self.openDetails = openDetails
        _with = State(initialValue: plain == nil ? (bots.first(where: \.bot.lead) ?? bots.first)?.ref : nil)
        _login = State(initialValue: Self.defaultLogin(on: plain))
    }

    /// Claude Code where it's ready on `plain`, else the first agent that is (the desktop's); no agent without one.
    private static func defaultLogin(on plain: BotStore?) -> AgentLogin {
        let seats = plain?.readySeats(on: AgentLogin(backend: "")) ?? []
        return (seats.first { $0.login == AgentLogin(backend: "claude") } ?? seats.first)?.login ?? AgentLogin(backend: "")
    }

    /// The bot it's with; nil for no bot, or for one that went away with its computer.
    private var bot: RosterItem? { with.flatMap { ref in bots.first { $0.ref == ref } } }
    /// The computer it starts on: its bot's, or without one `plain`.
    private var target: BotStore? { with == nil ? plain : bot.flatMap { store($0.ref.computerId) } }
    /// Whether it can go: to its bot while that bot is here, else without a bot on an agent.
    private var ready: Bool { with == nil ? !login.backend.isEmpty && plain != nil : target != nil }

    public var body: some View {
        if let started, let store = store(started.of.ref.computerId) {
            ThreadChatView(botId: started.of.bot.id, task: started.task)
                .environment(store)
        } else {
            page
        }
    }

    private var page: some View {
        intro
            .padding(.horizontal, 24)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Palette.background)
            .safeAreaInset(edge: .bottom, spacing: 0) { footer }
            .threadBar { EmptyView() }
            // A computer that starts threads without a bot came (or changed) while it's open: its own default agent.
            .onChange(of: plain?.computer.id) {
                login = Self.defaultLogin(on: plain)
                model = nil
                options = [:]
            }
    }

    /// Who it's with and "New thread with …", as the desktop's intro.
    @ViewBuilder private var intro: some View {
        let name = bot?.bot.name ?? (with == nil ? plain?.backendName(login.backend) : nil)
        VStack(spacing: 14) {
            if let bot {
                BotAvatar(bot: bot.bot, size: 72, animated: false)
            } else if with == nil, let plain {
                AgentIcon(registry: plain.agentRegistry(login.backend), size: 56)
            }
            if let name {
                Text(L10n.tr("New thread with {name}").replacingOccurrences(of: "{name}", with: name))
                    .font(.title2.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .multilineTextAlignment(.center)
                    .accessibilityAddTraits(.isHeader)
            }
        }
    }

    /// The chat's composer on the computer it starts on (any shown while its bot is gone), and why it didn't go.
    @ViewBuilder private var footer: some View {
        if let box = target ?? plain ?? bots.lazy.compactMap({ store($0.ref.computerId) }).first {
            VStack(alignment: .leading, spacing: 0) {
                Composer(botId: bot?.bot.id ?? "", replyingTo: .constant(nil),
                         start: ComposerStart(draftKey: Self.draftKey, ready: ready, controls: AnyView(controls), send: send))
                    .environment(box)
                    // One draft per computer: another computer's box loads its own.
                    .id(box.computer.id)
                if let error {
                    Text(L10n.tr(error))
                        .font(.caption)
                        .foregroundStyle(Palette.danger)
                        .textSelection(.enabled)
                        .padding(.horizontal, 24)
                        .padding(.bottom, 8)
                }
            }
        }
    }

    /// What it runs on, in the box's row: who it's with, then without a bot the model.
    private var controls: some View {
        HStack(spacing: 4) {
            withMenu
            if with == nil, let plain { modelChip(plain) }
        }
    }

    /// Who it's with: the bots (each with its computer once they're on several), then No bot where it can be.
    private var withMenu: some View {
        let many = Set(bots.map(\.ref.computerId)).count > 1
        return DropdownMenu {
            bots.map { item in
                MenuItem(many ? "\(item.bot.name) · \(store(item.ref.computerId)?.hostName ?? item.computer.name)" : item.bot.name,
                         selected: item.ref == with) { with = item.ref }
            } + (plain == nil ? [] : [MenuItem("No bot", icon: "app.dashed", selected: with == nil, divider: !bots.isEmpty) { with = nil }])
        } label: {
            ChipLabel {
                if let bot {
                    BotAvatar(bot: bot.bot, size: 18, animated: false)
                    Text(bot.bot.name)
                } else if with == nil {
                    NoBotAvatar(size: 18)
                    Text(L10n.tr("No bot"))
                } else {
                    Text(L10n.tr("Choose a bot"))
                }
            }
        }
        .accessibilityLabel(L10n.tr("Choose a bot"))
    }

    /// The agent and model a thread without a bot runs on; it opens the model panel.
    private func modelChip(_ plain: BotStore) -> some View {
        Button { choosingModel = true } label: {
            ChipLabel {
                AgentIcon(registry: plain.agentRegistry(login.backend), size: 16)
                Text(model ?? L10n.tr("Default"))
            }
        }
        .buttonStyle(PressScale())
        .accessibilityLabel(L10n.tr("Model"))
        .codyncSheet(isPresented: $choosingModel) {
            ModelPanel(backend: login.backend, loginProfile: login.loginProfile, model: $model, options: $options,
                       seats: plain.readySeats(on: login), pickSeat: { seat in
                           login = seat
                           model = nil
                           options = [:]
                       })
                .background(Palette.background)
                .environment(plain)
        }
    }

    /// Starts the thread; true once the computer took it. Its chat then takes this page's place.
    private func send(_ text: String) async -> Bool {
        let start: (store: BotStore, botId: String?, setup: ThreadSetup?)
        if with == nil, let plain {
            start = (plain, nil, .plain(login, home: plain.hello?.threadFolders == true ? nil : plain.hello?.home, model: model, options: options))
        } else if let bot, let botStore = store(bot.ref.computerId) {
            start = (botStore, bot.bot.id, nil)
        } else {
            return false
        }
        error = nil
        let botId: String, taskId: String
        do {
            (botId, taskId) = try await start.store.startThread(botId: start.botId, text: text, setup: start.setup)
        } catch {
            self.error = error.localizedDescription
            return false
        }
        start.store.setComposerDraft("", for: Self.draftKey)
        let computer = start.store.computer.id
        let task = await start.store.threadStarted(botId: botId, taskId: taskId)
        guard !Task.isCancelled, let item = item(computer, botId) else { return true }
        if let task {
            started = ThreadTarget(of: item, task: task)
        } else {
            dismiss()
            if !item.bot.plain { openDetails(item) }
        }
        return true
    }
}
