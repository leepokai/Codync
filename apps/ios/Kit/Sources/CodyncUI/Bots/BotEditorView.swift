import CodyncKit
import SwiftUI

public struct EditorRequest: Identifiable {
    public let id = UUID()
    public var draft: BotDraft

    public init(_ draft: BotDraft) { self.draft = draft }
}

/// Edit a bot in a modal (iPhone): the settings form plus Close / Save.
public struct BotEditorView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State var draft: BotDraft
    @State private var saving = false
    @State private var error: String?

    public init(draft: BotDraft) {
        _draft = State(initialValue: draft)
    }

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Settings") {
                if saving {
                    Spinner()
                } else {
                    IconButton("Save", systemImage: "checkmark") { save() }
                        .disabled(!draft.isValid || saving || model.connection != .online)
                }
            }
            BotSettingsForm(draft: $draft, error: error)
        }
        .background(Palette.background)
        // A change made elsewhere (a bot that manages the team, another device) replaces each field not edited here.
        .onChange(of: draft.id.flatMap { model.bots[$0] }.map(BotDraft.init)) { base, fresh in
            if let base, let fresh { draft = draft.rebased(from: base, onto: fresh) }
        }
    }

    private func save() {
        guard draft.isValid, !saving, model.connection == .online else { return }
        saving = true
        error = nil
        Task {
            do {
                _ = try await model.save(draft.normalized)
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
            saving = false
        }
    }
}

/// Bot settings, in sections in the Settings style (`CardSection`): what it is told and what runs it first.
/// `scrolls` is off where a surrounding view scrolls (the details sheet).
struct BotSettingsForm: View {
    @Binding var draft: BotDraft
    let error: String?
    var scrolls = true
    /// On the details sheet, whose Computer page has the computer's switch.
    var inDetails = false
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pickingFolder = false
    @State private var pickingAvatar = false
    @State private var viewingSkill: SkillRef?
    /// Retry model list reloads Permissions too: its modes come from the same list.
    @State private var modelReload = 0

    /// The saved bot, as the host has it now.
    private var bot: Bot? { draft.id.flatMap { model.bots[$0] } }

    var body: some View {
        Group {
            if scrolls {
                ScrollView { sections.padding(20) }
                    .scrollDismissesKeyboard(.interactively)
            } else {
                sections
            }
        }
        .font(.body)
        .background(Palette.background)
        .task { await model.refreshPlugins() }
        .codyncSheet(item: $viewingSkill) { SkillSheet(skill: $0) }
        .codyncSheet(isPresented: $pickingFolder) {
            FolderPicker(start: draft.cwd.isEmpty ? model.hello?.home : draft.cwd) {
                draft.cwd = $0
                pickingFolder = false
            }
        }
    }

    private var instructions: some View {
        Field("Standing instructions", detail: "Rules that always apply. Put task-specific requests in the chat instead.") {
            TextField(L10n.tr("e.g. Reviews PRs. Never pushes without asking."), text: $draft.description, axis: .vertical)
                .lineLimit(3...8)
                .fieldBox()
        }
    }

    private var modelPicker: some View {
        AgentModelPicker(backend: draft.backend, loginProfile: draft.loginProfile, selection: $draft.model, options: $draft.modelOptions,
                         fastMode: Binding(get: { draft.fastMode ?? false }, set: { on in
                             withAnimation(Motion.reduced(Motion.fade, reduceMotion)) { draft.fastMode = on }
                         }), bot: bot,
                         agents: model.loginSeats(model.hello?.backends ?? [], on: draft.login)
                             + [LoginSeat(login: AgentLogin(backend: "custom"), name: L10n.tr("Custom command"))],
                         onAgentChange: { draft.setAgent($0) },
                         command: $draft.command,
                         installHint: model.hello?.backends.first { $0.id == draft.backend && !$0.available }?.installHint,
                         restLine: bot?.rest(of: draft.login).map { L10n.tr($0.line) }, onReload: { modelReload += 1 })
    }

    /// While the bot runs on a fallback: which, and Switch back.
    @ViewBuilder private var fallbackBanner: some View {
        if let bot, let step = bot.agentStep { FallbackBanner(bot: bot, step: step) }
    }

    /// Only from a host that has fallbacks (its bots say).
    @ViewBuilder private var fallbackList: some View {
        if draft.fallbacks != nil {
            FallbackList(draft: $draft, bot: bot)
            SwitchRow("Let the bot change its fallbacks", detail: "It can set this list itself, for example when you ask it to.",
                      isOn: Binding(get: { draft.selfFallbacks ?? true }, set: { draft.selfFallbacks = $0 }))
        }
    }

    /// Advisor mode, from a host that has it: its threads' agent and fallbacks show once it's on, in its row (one
    /// setting, no lines inside it).
    @ViewBuilder private var advisor: some View {
        if draft.threadFallbacks != nil {
            VStack(alignment: .leading, spacing: 16) {
                SwitchRow("Advisor mode", detail: "The chat plans and hands work on; threads run on an agent of their own, with its own fallbacks.",
                          isOn: Binding(get: { draft.threadAgent != nil }, set: { on in
                              withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { draft.setAdvisor(on, backends: model.hello?.backends ?? []) }
                          }))
                if draft.threadAgent != nil {
                    ThreadsAgentRow(draft: $draft, bot: bot)
                    FallbackList(draft: $draft, bot: bot, threads: true)
                }
            }
        }
    }

    /// Only where the host summarizes this bot's chat (its bot says).
    @ViewBuilder private var compactionPicker: some View {
        if bot?.compaction != nil, draft.compaction != nil {
            CompactionPicker(selection: Binding(get: { draft.compaction ?? .off }, set: { draft.compaction = $0 }),
                             range: bot?.compactionTokens)
        }
    }

    /// The bot's one folder, on a host without projects: its own, or a project the user picks.
    @ViewBuilder private var folderPicker: some View {
        ValueRow("Workspace", detail: draft.cwd.isEmpty
                 ? "This bot has its own space for files. It can work in other folders when you ask."
                 : "This project is the default starting folder. The bot can work elsewhere when you ask.") {
            DropdownMenu {
                [
                    MenuItem("Own folder", selected: draft.cwd.isEmpty) {
                        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { draft.cwd = "" }
                    },
                    MenuItem("Choose project folder…", selected: !draft.cwd.isEmpty) {
                        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { pickingFolder = true }
                    },
                ]
            } label: {
                HStack(spacing: 6) {
                    Text(draft.cwd.isEmpty ? L10n.tr("Own folder") : (draft.cwd as NSString).lastPathComponent).lineLimit(1)
                    Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).foregroundStyle(Palette.secondary)
                }
                .foregroundStyle(Palette.text)
                .outlinedPill()
            }
            .help(draft.cwd.isEmpty ? "A persistent workspace allocated for this bot" : draft.cwd)
        }
        // From a host that says, for any folder (one Codync made too).
        if let bot, bot.threadWorktrees != nil {
            GitStateRow(state: bot.gitState)
        }
    }

    private var sections: some View {
        VStack(alignment: .leading, spacing: 30) {
            CardSection("Instructions", fold: "bot.instructions") { instructions }
            CardSection("Agent", fold: "bot.agent") {
                fallbackBanner
                modelPicker
                fallbackList
                advisor
                compactionPicker
            }
            .animation(Motion.reduced(Motion.layout, reduceMotion), value: bot?.agentStep)

            CardSection("Profile", fold: "bot.profile") {
                VStack(alignment: .leading, spacing: 12) {
                    Button {
                        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { pickingAvatar.toggle() }
                    } label: {
                        ValueRow("Avatar") {
                            CharacterAvatar(shape: draft.avatarShape, color: draft.avatarColor, size: 28, customIcon: draft.id.flatMap { model.icons["bot:\($0)"] })
                                .padding(5)
                                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Edit Bot avatar")
                    if pickingAvatar {
                        VStack(spacing: 16) {
                            AvatarPicker(shape: $draft.avatarShape, color: $draft.avatarColor)
                            if let id = draft.id { CustomIconEditor(target: "bot:\(id)") }
                        }
                            .transition(.opacity.combined(with: .move(edge: .top)))
                    }
                }
                Field("Name") {
                    TextField("Name", text: $draft.name).fieldBox()
                }
                if let bot, let creator = bot.createdBy {
                    ValueRow("Created by", value: "\(model.bots[creator]?.name ?? "A bot") · \(Date(milliseconds: bot.createdAt).formatted(date: .abbreviated, time: .omitted))")
                }
            }

            CardSection("Workspace", fold: "bot.workspace") {
                ValueRow("Computer") {
                    Text(model.hostName).lineLimit(1)
                }
                if model.hello?.projects == true {
                    BotProjects(draft: $draft, bot: bot)
                } else {
                    folderPicker
                }
                ApprovalPicker(draft: $draft, reload: modelReload)
                // Only from a host with a box (its bots say).
                if let threads = draft.maxThreads {
                    ValueRow("Threads at once") {
                        HStack(spacing: 10) {
                            Text(verbatim: "\(threads)").monospacedDigit().foregroundStyle(Palette.text)
                            Stepper(L10n.tr("Threads at once"), value: Binding(get: { threads }, set: { draft.maxThreads = $0 }),
                                    in: Bot.maxThreadsRange)
                                .labelsHidden()
                        }
                    }
                }
                // Only in a project folder in a git repository (a bot's own folder keeps all its work in one place),
                // from a host that has the setting.
                if model.hello?.projects != true, bot?.gitRepo == true, bot?.managedWorkspace == false, let worktrees = draft.threadWorktrees {
                    SwitchRow("Worktree per thread", detail: "Threads that work at once in this repository each get their own branch.",
                              isOn: Binding(get: { worktrees }, set: { draft.threadWorktrees = $0 }))
                }
            }

            CardSection("Activity", fold: "bot.activity") {
                SwitchRow("Notifications", detail: "Get notified when this bot finishes or needs you",
                          isOn: Binding(get: { draft.notify ?? true }, set: { draft.notify = $0 }))
                if model.screen != nil, !inDetails {
                    SwitchRow("Use the computer", detail: model.screen?.controlReady != true
                        ? "Let this bot use apps. On that computer, open Codync → Settings → Computer access to set up permissions first."
                        : model.screen?.enabled == true
                        ? "Let this bot see the screen and use the mouse and keyboard. You can watch and take over from your phone."
                        : model.screen?.computerUse == true
                        ? "Let this bot see the screen and use the mouse and keyboard."
                        : "Let this bot see the screen and use the mouse and keyboard. On that computer, open Codync → Settings → Computer access to set up permissions first.",
                              isOn: Binding(get: { draft.computer ?? false }, set: { draft.computer = $0 }))
                }
                // Only from a host that has it (its bots say).
                if model.bots.values.contains(where: { $0.manageTeam != nil }) {
                    SwitchRow("Manage the team", detail: "Let this bot create bots, change any bot's setup including its model and fallbacks, and pause or resume it. The bots it creates can too.",
                              isOn: Binding(get: { draft.manageTeam ?? false }, set: { draft.manageTeam = $0 }))
                }
                // Each only where it applies: the computer is in a network, cloud assistants are on.
                if draft.offeredToNetwork != nil, model.network?.joined == true {
                    SwitchRow("Offered to the network", detail: "Bots on your other computers can see and message this bot.",
                              isOn: Binding(get: { draft.offeredToNetwork ?? true }, set: { draft.offeredToNetwork = $0 }))
                }
                if draft.reachableByAssistants != nil, model.network?.assistants == true {
                    SwitchRow("Reachable by cloud assistants", detail: "ChatGPT or Claude in your Codync account can see and message this bot.",
                              isOn: Binding(get: { draft.reachableByAssistants ?? true }, set: { draft.reachableByAssistants = $0 }))
                }
            }

            PluginToggles(
                title: "Connectors",
                empty: "No connectors yet. Add GitHub, Linear, Notion and more from Extensions.",
                items: model.installedConnectors.map { ($0.id, $0.name, $0.command ?? $0.url ?? "") },
                selection: Binding(get: { draft.connectors ?? [] }, set: { draft.connectors = $0 })
            )
            PluginToggles(
                title: "Skills",
                empty: "No skills yet. Get some from Extensions, or write your own.",
                items: model.installedSkills.map { ($0.id, $0.name, "") },
                selection: Binding(get: { draft.skills ?? [] }, set: { draft.skills = $0 }),
                info: { id in viewingSkill = model.installedSkills.first { $0.id == id }.map(SkillRef.init) }
            )
            if let error {
                Text(L10n.tr(error)).font(.footnote).foregroundStyle(Palette.danger)
            }
        }
    }
}

/// A labeled input inside a section row: label (and an optional note) over the field.
struct Field<Content: View>: View {
    let label: String
    var detail: String?
    @ViewBuilder let content: Content
    @Environment(\.noteLines) private var noteLines

    init(_ label: String, detail: String? = nil, @ViewBuilder content: () -> Content) {
        self.label = label
        self.detail = detail
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            VStack(alignment: .leading, spacing: 3) {
                Text(L10n.tr(label)).foregroundStyle(Palette.text)
                if let detail {
                    Text(L10n.tr(detail)).font(.caption).foregroundStyle(Palette.secondary).lineSpacing(2)
                        .lineLimit(noteLines)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            content
        }
    }
}

/// An on/off row: title and a short note, the switch on the right.
struct SwitchRow: View {
    let title: String
    var detail: String?
    @Binding var isOn: Bool
    @Environment(\.noteLines) private var noteLines

    init(_ title: String, detail: String? = nil, isOn: Binding<Bool>) {
        self.title = title
        self.detail = detail
        _isOn = isOn
    }

    var body: some View {
        Toggle(isOn: $isOn) {
            VStack(alignment: .leading, spacing: 3) {
                Text(L10n.tr(title)).foregroundStyle(Palette.text)
                if let detail {
                    Text(L10n.tr(detail)).font(.caption).foregroundStyle(Palette.secondary).lineSpacing(2)
                        .lineLimit(noteLines)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .toggleStyle(.codync)
    }
}

extension View {
    /// A rounded input box: the page color in a filled card, filled in an outlined one.
    func fieldBox(fill: Color? = nil) -> some View {
        modifier(FieldBox(fill: fill))
    }
}

private struct FieldBox: ViewModifier {
    let fill: Color?
    @Environment(\.cardStyle) private var cardStyle

    func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .padding(.horizontal, 14)
            .padding(.vertical, 11)
            .background(fill ?? (cardStyle == .outlined ? Palette.surface : Palette.background),
                        in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

extension BotDraft {
    /// A saved bot keeps a name; only a new one goes without (the host names it after its first conversations).
    var isValid: Bool { !name.trimmingCharacters(in: .whitespaces).isEmpty }

    /// What the host should get: no stale command unless the agent is custom.
    var normalized: BotDraft {
        var d = self
        if d.backend != "custom" { d.command = nil }
        return d
    }
}
