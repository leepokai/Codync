import CodyncKit
import SwiftUI

public struct EditorRequest: Identifiable {
    public let id = UUID()
    public var draft: BotDraft

    public init(_ draft: BotDraft) { self.draft = draft }
}

/// Create or edit a bot in a modal (iPhone): the settings form plus Close / Save.
public struct BotEditorView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State var draft: BotDraft
    private let computers: [(id: ComputerID, label: String)]
    private let computer: Binding<ComputerID>?

    /// `computer` lets a new bot pick which computer it runs on; the caller swaps the `BotStore` to match.
    public init(draft: BotDraft, computers: [(id: ComputerID, label: String)] = [], computer: Binding<ComputerID>? = nil) {
        _draft = State(initialValue: draft)
        self.computers = computers
        self.computer = computer
    }
    @State private var saving = false
    @State private var error: String?

    private var isNew: Bool { draft.id == nil }

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader(isNew ? "New bot" : "Settings") {
                if saving {
                    Spinner()
                } else {
                    IconButton(isNew ? "Create" : "Save", systemImage: "checkmark") { save() }
                        .disabled(!draft.isValid || saving || model.connection != .online)
                }
            }
            BotSettingsForm(draft: $draft, error: error, computers: computers, computer: computer)
        }
        .background(Palette.background)
        .onAppear { if isNew { model.fillDefaults(&draft) } }
        .onChange(of: model.computer.id) {
            // Folders, models and connectors belong to the old computer.
            draft.cwd = ""
            draft.model = nil
            draft.connectors = nil
            draft.skills = nil
            model.fillDefaults(&draft)
        }
    }

    private func save() {
        guard draft.isValid, !saving, model.connection == .online else { return }
        saving = true
        error = nil
        Task {
            do {
                let bot = try await model.save(draft.normalized)
                dismiss()
                if isNew { model.selection = bot.id }
            } catch {
                self.error = error.localizedDescription
            }
            saving = false
        }
    }
}

/// Bot settings: a big avatar (Grok Bot's), then sections in the Settings style (`CardSection`).
struct BotSettingsForm: View {
    @Binding var draft: BotDraft
    let error: String?
    var computers: [(id: ComputerID, label: String)] = []
    var computer: Binding<ComputerID>?
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pickingFolder = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 30) {
                VStack(spacing: 16) {
                    CharacterAvatar(shape: draft.avatarShape, color: draft.avatarColor, size: 96)
                        .padding(18)
                        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
                    AvatarPicker(shape: $draft.avatarShape, color: $draft.avatarColor)
                }
                .frame(maxWidth: .infinity)

                CardSection("Profile") {
                    // A new bot is named from its first conversations (Grok Bot's flow); rename it any time after.
                    if draft.id != nil {
                        Field("Name") {
                            TextField("Name", text: $draft.name).fieldBox()
                        }
                    }
                    Field("Standing instructions", detail: "Rules that always apply. Put task-specific requests in the chat instead.") {
                        TextField("e.g. Reviews PRs. Never pushes without asking.", text: $draft.description, axis: .vertical)
                            .lineLimit(3...8)
                            .fieldBox()
                    }
                }

                CardSection("Agent") {
                    VStack(alignment: .leading, spacing: 6) {
                        ValueRow("Computer") {
                            if let computer {
                                ChoicePicker(selection: computer, options: computers, fitsAvailableWidth: true)
                                    .accessibilityLabel("Computer")
                            } else {
                                Text(model.hostName).lineLimit(1)
                            }
                        }
                        if computer != nil, model.connection != .online {
                            Text("Connect this computer to create the bot, or choose another computer.")
                                .font(.caption).foregroundStyle(Palette.warning)
                        }
                    }
                    VStack(alignment: .leading, spacing: 8) {
                        ValueRow("Agent") {
                            ChoicePicker(selection: $draft.backend,
                                         options: (model.hello?.backends ?? []).map { ($0.id, $0.available ? $0.name : "\($0.name) (not installed)") } + [("custom", "Custom command")])
                        }
                        if let b = model.hello?.backends.first(where: { $0.id == draft.backend }), !b.available {
                            Text(b.installHint).font(.caption).foregroundStyle(Palette.warning)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        if draft.backend == "custom" {
                            TextField("ACP command, e.g. my-agent --acp", text: Binding(get: { draft.command ?? "" }, set: { draft.command = $0.isEmpty ? nil : $0 }))
                                .font(.callout.monospaced())
                                .plainTextInput()
                                .fieldBox()
                        }
                    }
                    AgentModelPicker(backend: draft.backend, selection: $draft.model)
                    ValueRow("Workspace", detail: draft.cwd.isEmpty
                             ? "This bot has its own space for files. It can work in other folders when you ask."
                             : "This project is the default starting folder. The bot can work elsewhere when you ask.") {
                        DropdownMenu {
                            [
                                MenuItem("Personal workspace", selected: draft.cwd.isEmpty) {
                                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { draft.cwd = "" }
                                },
                                MenuItem("Choose project folder…", selected: !draft.cwd.isEmpty) {
                                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { pickingFolder = true }
                                },
                            ]
                        } label: {
                            HStack(spacing: 6) {
                                Text(draft.cwd.isEmpty ? "Personal" : (draft.cwd as NSString).lastPathComponent).lineLimit(1)
                                Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).foregroundStyle(Palette.secondary)
                            }
                            .foregroundStyle(Palette.text)
                            .outlinedPill()
                        }
                        .help(draft.cwd.isEmpty ? "A persistent workspace allocated for this bot" : draft.cwd)
                    }
                    ValueRow("Permissions", detail: draft.permission == "auto"
                             ? "Tool requests are approved automatically. Your agent's own settings (like Claude Code's permission rules) still apply."
                             : "You'll get an approval card and a notification whenever the agent asks.") {
                        ChoicePicker(selection: $draft.permission, options: [("ask", "Ask me"), ("auto", "Approve automatically")])
                    }
                }

                CardSection("Activity") {
                    SwitchRow("Notifications", detail: "Get notified when this bot finishes or needs you",
                              isOn: Binding(get: { draft.notify ?? true }, set: { draft.notify = $0 }))
                    if model.screen != nil {
                        SwitchRow("Use the computer", detail: model.screen?.controlReady != true
                            ? "Let this bot use apps. On that computer, open Codync → Settings → Computer access to set up permissions first."
                            : model.screen?.enabled == true
                            ? "Let this bot see the screen and use the mouse and keyboard. You can watch and take over from your phone."
                            : model.screen?.computerUse == true
                            ? "Let this bot see the screen and use the mouse and keyboard."
                            : "Let this bot see the screen and use the mouse and keyboard. On that computer, open Codync → Settings → Computer access to set up permissions first.",
                                  isOn: Binding(get: { draft.computer ?? false }, set: { draft.computer = $0 }))
                    }
                }

                PluginToggles(
                    title: "Connectors",
                    empty: "No connectors yet. Add GitHub, Linear, Notion and more from Plugins.",
                    items: model.installedConnectors.map { ($0.id, $0.name, $0.command ?? $0.url ?? "") },
                    selection: Binding(get: { draft.connectors ?? [] }, set: { draft.connectors = $0 })
                )
                PluginToggles(
                    title: "Skills",
                    empty: "No skills yet. Get some from Plugins, or write your own.",
                    items: model.installedSkills.map { ($0.id, $0.name, $0.description) },
                    selection: Binding(get: { draft.skills ?? [] }, set: { draft.skills = $0 })
                )
                if let id = draft.id {
                    MemoryCard(botId: id)
                }

                if let error {
                    Text(error).font(.footnote).foregroundStyle(Palette.danger)
                }
            }
            .padding(20)
        }
        .font(.body)
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
        .task { await model.refreshPlugins() }
        .codyncSheet(isPresented: $pickingFolder) {
            FolderPicker(start: draft.cwd.isEmpty ? model.hello?.home : draft.cwd) {
                draft.cwd = $0
                pickingFolder = false
            }
        }
    }
}

/// A labeled input inside a section row: label (and an optional note) over the field.
struct Field<Content: View>: View {
    let label: String
    var detail: String?
    @ViewBuilder let content: Content

    init(_ label: String, detail: String? = nil, @ViewBuilder content: () -> Content) {
        self.label = label
        self.detail = detail
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            VStack(alignment: .leading, spacing: 3) {
                Text(label).foregroundStyle(Palette.text)
                if let detail {
                    Text(detail).font(.caption).foregroundStyle(Palette.secondary).lineSpacing(2)
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

    init(_ title: String, detail: String? = nil, isOn: Binding<Bool>) {
        self.title = title
        self.detail = detail
        _isOn = isOn
    }

    var body: some View {
        Toggle(isOn: $isOn) {
            VStack(alignment: .leading, spacing: 3) {
                Text(title).foregroundStyle(Palette.text)
                if let detail {
                    Text(detail).font(.caption).foregroundStyle(Palette.secondary).lineSpacing(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .toggleStyle(.codync)
    }
}

/// A card of on/off switches for the connectors or skills installed on the computer.
private struct PluginToggles: View {
    let title: String
    let empty: String
    let items: [(id: String, name: String, detail: String)]
    @Binding var selection: [String]

    var body: some View {
        CardSection(title) {
            if items.isEmpty {
                Text(empty).foregroundStyle(Palette.secondary)
            }
            ForEach(items, id: \.id) { item in
                SwitchRow(item.name, detail: item.detail.isEmpty ? nil : item.detail, isOn: Binding(
                    get: { selection.contains(item.id) },
                    set: { on in
                        selection.removeAll { $0 == item.id }
                        if on { selection.append(item.id) }
                    }
                ))
            }
        }
    }
}

extension View {
    /// A rounded input box.
    func fieldBox(fill: Color = Palette.background) -> some View {
        textFieldStyle(.plain)
            .padding(.horizontal, 14)
            .padding(.vertical, 11)
            .background(fill, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

extension BotDraft {
    /// A new bot may go without a name: the host names it after its first conversations.
    var isValid: Bool { id == nil || !name.trimmingCharacters(in: .whitespaces).isEmpty }

    /// What the host should get: no stale command unless the agent is custom.
    var normalized: BotDraft {
        var d = self
        if d.backend != "custom" { d.command = nil }
        return d
    }
}

public extension BotStore {
    /// Picks an installed agent; empty cwd requests a personal workspace from the host.
    func fillDefaults(_ draft: inout BotDraft) {
        if hello?.backends.first(where: { $0.id == draft.backend })?.available != true,
           let first = hello?.backends.first(where: \.available) {
            draft.backend = first.id
        }
        // Connectors start on; left unset, the host turns on every one.
        if draft.connectors == nil, !installedConnectors.isEmpty {
            draft.connectors = installedConnectors.map(\.id)
        }
    }
}

struct AvatarPicker: View {
    @Binding var shape: String
    @Binding var color: String

    var body: some View {
        VStack(spacing: 12) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 10) {
                    ForEach(AvatarPalette.shapes, id: \.self) { s in
                        CharacterAvatar(shape: s, color: color, size: 36)
                            .padding(4)
                            .background(Circle().strokeBorder(s == shape ? Palette.accent : .clear, lineWidth: 2))
                            .onTapGesture { withAnimation(Motion.hover) { shape = s } }
                            .accessibilityLabel("\(s) shape")
                            .accessibilityAddTraits(s == shape ? .isSelected : [])
                    }
                }
                .padding(4)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 10) {
                    ForEach(AvatarPalette.colors) { c in
                        Circle()
                            .fill(c.color)
                            .frame(width: 26, height: 26)
                            .padding(4)
                            .background(Circle().strokeBorder(c.id == color ? Palette.accent : .clear, lineWidth: 2))
                            .onTapGesture { withAnimation(Motion.hover) { color = c.id } }
                            .accessibilityLabel(c.label)
                            .accessibilityAddTraits(c.id == color ? .isSelected : [])
                    }
                }
                .padding(4)
            }
        }
    }
}

/// Browses folders on the host, drilling down in place.
struct FolderPicker: View {
    let start: String?
    let onPick: (String) -> Void
    /// The folders opened so far; the last one is shown.
    @State private var stack: [String?]
    @State private var forward = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    init(start: String?, onPick: @escaping (String) -> Void) {
        self.start = start
        self.onPick = onPick
        _stack = State(initialValue: [start])
    }

    var body: some View {
        let current = stack.last ?? nil
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                if stack.count > 1 {
                    BackButton { go(forward: false) { stack.removeLast() } }
                        .padding(.leading, 12)
                        .transition(.opacity)
                }
                ModalHeader(current.map { ($0 as NSString).lastPathComponent } ?? "Folders")
            }
            ZStack {
                FolderLevel(path: current, onPick: onPick) { path in go(forward: true) { stack.append(path) } }
                    .id(stack.count)
                    .transition(.asymmetric(
                        insertion: .move(edge: forward ? .trailing : .leading),
                        removal: .move(edge: forward ? .leading : .trailing)
                    ).combined(with: .opacity))
            }
            .frame(maxHeight: .infinity, alignment: .top)
            .clipped()
        }
        .background(Palette.background)
    }

    /// Sets the slide direction first, so the leaving level takes it too, then moves.
    private func go(forward: Bool, _ change: @escaping () -> Void) {
        self.forward = forward
        Task { @MainActor in
            withAnimation(Motion.reduced(Motion.layout, reduceMotion), change)
        }
    }
}

/// One folder's subfolders.
private struct FolderLevel: View {
    let path: String?
    let onPick: (String) -> Void
    let open: (String) -> Void
    @Environment(BotStore.self) private var model
    @State private var listing: DirListing?
    @State private var error: String?
    @State private var filter = ""

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 22) {
                SearchField(text: $filter)
                if let listing {
                    CardSection(footer: listing.path) {
                        Button {
                            onPick(listing.path)
                        } label: {
                            Label("Use “\((listing.path as NSString).lastPathComponent)”", systemImage: "checkmark.circle.fill")
                                .font(.headline)
                                .foregroundStyle(Palette.accent)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(PressScale())
                    }
                    let dirs = listing.dirs.filter { filter.isEmpty || $0.name.localizedCaseInsensitiveContains(filter) }
                    if !dirs.isEmpty {
                        CardSection {
                            ForEach(dirs) { dir in
                                Button {
                                    open(dir.path)
                                } label: {
                                    HStack(spacing: 10) {
                                        Image(systemName: dir.isGit ? "arrow.triangle.branch" : "folder")
                                            .foregroundStyle(dir.isGit ? Palette.accent : Palette.secondary)
                                            .frame(width: 20)
                                        Text(dir.name).foregroundStyle(Palette.text).lineLimit(1)
                                        Spacer(minLength: 8)
                                        Image(systemName: "chevron.right")
                                            .font(.caption2.weight(.semibold))
                                            .foregroundStyle(Palette.tertiary)
                                    }
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(PressScale())
                            }
                        }
                    }
                } else if let error {
                    Text(error).foregroundStyle(Palette.danger)
                } else {
                    Spinner(size: 20).frame(maxWidth: .infinity)
                }
            }
            .font(.body)
            .padding(20)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
        .task {
            do { listing = try await model.listDirs(path) } catch { self.error = error.localizedDescription }
        }
    }
}
