import CodyncKit
import SwiftUI

/// A thread the user starts from the Threads tab, as the desktop's New thread does. It opens on no bot, on the first
/// computer that starts one that way, and otherwise on the lead bot. It has a first message, then who it's with under
/// the field: a bot from `bots` (on that bot's own model, approvals and folder) or **No bot**. Without a bot, it runs
/// on the agent and model the chip beside it picks (the desktop's model panel), in a folder of its own (an older host: the computer's home folder), asking
/// before anything it does, in the agent's Auto mode. `started` gets the bot it went to and the task's id once the
/// computer took it and the sheet is gone, so what it opens doesn't push while the sheet still slides away (a push then
/// lands blank).
public struct NewThreadSheet: View {
    let bots: [RosterItem]
    /// The computer a thread without a bot starts on; nil where none can.
    let plain: BotStore?
    let store: (ComputerID) -> BotStore?
    let started: (_ computer: ComputerID, _ botId: String, _ taskId: String) -> Void
    @Environment(\.dismissModal) private var dismiss
    /// The bot it's with; nil for no bot.
    @State private var with: BotReference?
    @State private var login: AgentLogin
    @State private var model: String?
    @State private var options: [String: String] = [:]
    @State private var choosingModel = false
    @State private var text = ""
    @State private var sending = false
    @State private var error: String?
    /// What the computer took, handed to `started` once the sheet has slid away.
    @State private var took: (computer: ComputerID, botId: String, taskId: String)?
    @FocusState private var focused: Bool

    public init(bots: [RosterItem], plain: BotStore?, store: @escaping (ComputerID) -> BotStore?,
                started: @escaping (_ computer: ComputerID, _ botId: String, _ taskId: String) -> Void) {
        self.bots = bots
        self.plain = plain
        self.store = store
        self.started = started
        _with = State(initialValue: plain == nil ? (bots.first(where: \.bot.lead) ?? bots.first)?.ref : nil)
        _login = State(initialValue: Self.defaultLogin(on: plain))
    }

    /// Claude Code where it's ready on `plain`, else the first agent that is (the desktop's); no agent without one.
    private static func defaultLogin(on plain: BotStore?) -> AgentLogin {
        let seats = plain?.readySeats(on: AgentLogin(backend: "")) ?? []
        return (seats.first { $0.login == AgentLogin(backend: "claude") } ?? seats.first)?.login ?? AgentLogin(backend: "")
    }

    private var trimmed: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }
    /// The bot it's with; nil for no bot, or for one that went away with its computer.
    private var bot: RosterItem? { with.flatMap { ref in bots.first { $0.ref == ref } } }
    /// Whether it can go: to its bot while that bot is here, else without a bot on an agent.
    private var ready: Bool {
        !trimmed.isEmpty && (with == nil ? !login.backend.isEmpty && plain != nil : bot != nil)
    }

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader(title: { Text(L10n.tr("New thread")) }) {
                if sending {
                    Spinner()
                } else {
                    IconButton("Send", systemImage: "arrow.up", action: send)
                        .disabled(!ready)
                }
            }
            CardForm {
                CardSection {
                    TextField(L10n.tr("What should it work on?"), text: $text, axis: .vertical)
                        .lineLimit(3...10)
                        .focused($focused)
                }
                HStack(spacing: 4) {
                    withMenu
                    if with == nil, let plain { modelChip(plain) }
                    Spacer(minLength: 0)
                }
                if let error {
                    Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
                }
            }
            .textFieldStyle(.plain)
        }
        .background(Palette.background)
        .presentationDetents([.medium, .large])
        .onAppear { focused = true }
        // A computer that starts threads without a bot came (or changed) while it's open: its own default agent.
        .onChange(of: plain?.computer.id) {
            login = Self.defaultLogin(on: plain)
            model = nil
            options = [:]
        }
        .onDisappear { if let took { started(took.computer, took.botId, took.taskId) } }
    }

    /// Who it's with: the bots (each with its computer once they're on several), then No bot where it can be.
    private var withMenu: some View {
        let many = Set(bots.map(\.ref.computerId)).count > 1
        return DropdownMenu {
            bots.map { item in
                MenuItem(many ? "\(item.bot.name) · \(store(item.ref.computerId)?.hostName ?? item.computer.name)" : item.bot.name,
                         selected: item.ref == with) { with = item.ref }
            } + (plain == nil ? [] : [MenuItem("No bot", icon: "person.crop.circle.dashed", selected: with == nil, divider: !bots.isEmpty) { with = nil }])
        } label: {
            ChipLabel {
                if let bot {
                    BotAvatar(bot: bot.bot, size: 18, animated: false)
                    Text(bot.bot.name)
                } else if with == nil {
                    Image(systemName: "person.crop.circle.dashed")
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

    private func send() {
        guard !sending, ready else { return }
        let target: (BotStore, String?, ThreadSetup?)
        if with == nil, let plain {
            target = (plain, nil, .plain(login, home: plain.hello?.threadFolders == true ? nil : plain.hello?.home, model: model, options: options))
        } else if let bot, let botStore = store(bot.ref.computerId) {
            target = (botStore, bot.bot.id, nil)
        } else {
            return
        }
        sending = true
        error = nil
        Task {
            do {
                let (botId, taskId) = try await target.0.startThread(botId: target.1, text: trimmed, setup: target.2)
                took = (target.0.computer.id, botId, taskId)
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
            sending = false
        }
    }
}
