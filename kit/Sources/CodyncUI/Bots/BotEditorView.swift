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

    public init(draft: BotDraft) { _draft = State(initialValue: draft) }
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
                        .disabled(!draft.isValid)
                }
            }
            BotSettingsForm(draft: $draft, error: error)
        }
        .background(Palette.background)
        .onAppear { if isNew { model.fillDefaults(&draft) } }
    }

    private func save() {
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

/// The desktop inspector next to a conversation: the same form, saved as you edit.
public struct BotSettingsPanel: View {
    let botId: String
    @Environment(BotStore.self) private var model
    @State private var draft: BotDraft?
    @State private var error: String?

    public init(botId: String) { self.botId = botId }

    public var body: some View {
        Group {
            if let binding = Binding($draft) {
                BotSettingsForm(draft: binding, error: error)
            } else {
                Color.clear
            }
        }
        .task(id: botId) {
            draft = model.bots[botId].map(BotDraft.init)
        }
        .task(id: draft) {
            // Autosave a moment after the last change.
            guard let draft, let bot = model.bots[botId], draft != BotDraft(bot), draft.isValid else { return }
            try? await Task.sleep(for: .milliseconds(600))
            guard !Task.isCancelled else { return }
            do {
                _ = try await model.save(draft.normalized)
                error = nil
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}

/// Grok Bot-style bot settings: a big avatar, labeled fields, and one card of options.
struct BotSettingsForm: View {
    @Binding var draft: BotDraft
    let error: String?
    @Environment(BotStore.self) private var model
    @State private var pickingFolder = false
    @State private var pickingAvatar = false
    @State private var avatarFrame: CGRect = .zero

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: InterfaceMetrics.value(mac: 12, mobile: 18)) {
                VStack(spacing: InterfaceMetrics.value(mac: 12, mobile: 16)) {
                    #if os(macOS)
                    Button { pickingAvatar.toggle() } label: {
                        CharacterAvatar(shape: draft.avatarShape, color: draft.avatarColor, size: 56)
                            .padding(12)
                            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 20))
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Edit Bot avatar")
                    .help("Edit Bot avatar")
                    .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { avatarFrame = $0 }
                    .codyncOverlay(isPresented: $pickingAvatar) { close in
                        AnchoredPanel(anchor: avatarFrame, close: close) {
                            AvatarPicker(shape: $draft.avatarShape, color: $draft.avatarColor)
                                .padding(14)
                                .frame(width: 300)
                        }
                    }
                    #else
                    CharacterAvatar(shape: draft.avatarShape, color: draft.avatarColor, size: 96)
                        .padding(18)
                        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
                    AvatarPicker(shape: $draft.avatarShape, color: $draft.avatarColor)
                    #endif
                }
                .frame(maxWidth: .infinity)

                Field("Name") {
                    TextField("Name", text: $draft.name).fieldBox()
                }
                Field("Standing instructions") {
                    TextField("e.g. Reviews PRs. Never pushes without asking.", text: $draft.description, axis: .vertical)
                        .lineLimit(3...8)
                        .fieldBox()
                    Text("Rules that always apply. Put task-specific requests in the chat instead.")
                        .font(.caption)
                        .foregroundStyle(Palette.tertiary)
                }

                // Grok-style option card: no dividers, values in outlined pills.
                VStack(alignment: .leading, spacing: InterfaceMetrics.value(mac: 12, mobile: 20)) {
                    OptionRow("Computer") {
                        Text(model.hostName)
                            .lineLimit(1)
                            .foregroundStyle(Palette.secondary)
                    }
                    OptionRow("Agent") {
                        ChoicePicker(selection: $draft.backend,
                                 options: (model.hello?.backends ?? []).map { ($0.id, $0.available ? $0.name : "\($0.name) (not installed)") } + [("custom", "Custom command")],
                                 fill: Palette.background)
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
                    OptionRow("Model") {
                        TextField("Default", text: Binding(get: { draft.model ?? "" }, set: { draft.model = $0.isEmpty ? nil : $0 }))
                            .plainTextInput()
                            .multilineTextAlignment(.trailing)
                            .textFieldStyle(.plain)
                            .frame(width: InterfaceMetrics.value(mac: 100, mobile: 130))
                            .pill()
                    }
                    OptionRow("Project folder") {
                        Button { pickingFolder = true } label: {
                            HStack(spacing: 6) {
                                Text(draft.cwd.isEmpty ? "Choose" : (draft.cwd as NSString).lastPathComponent).lineLimit(1)
                                Image(systemName: "chevron.right").font(.caption2.weight(.semibold)).foregroundStyle(Palette.secondary)
                            }
                            .pill()
                        }
                        .buttonStyle(.plain)
                        .help(draft.cwd)
                    }
                    OptionRow("Permissions") {
                        ChoicePicker(selection: $draft.permission, options: [("ask", "Ask me"), ("auto", "Approve automatically")],
                                 fill: Palette.background)
                    }
                    Toggle(isOn: Binding(get: { draft.notify ?? true }, set: { draft.notify = $0 })) {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("Notifications").foregroundStyle(Palette.text)
                            Text("Get notified when this bot finishes or needs you").font(InterfaceMetrics.secondary).foregroundStyle(Palette.secondary)
                        }
                    }
                    .toggleStyle(.codync)
                    if model.screen != nil {
                        Toggle(isOn: Binding(get: { draft.computer ?? false }, set: { draft.computer = $0 })) {
                            VStack(alignment: .leading, spacing: 3) {
                                Text("Use the computer").foregroundStyle(Palette.text)
                                Text(model.screen?.enabled == true
                                    ? "Let this bot see the screen and use the mouse and keyboard. You can watch and take over from your phone."
                                    : "Let this bot see the screen and use the mouse and keyboard. Turn on Remote screen in Codync's menu on the computer first.")
                                    .font(InterfaceMetrics.secondary).foregroundStyle(Palette.secondary)
                            }
                        }
                        .toggleStyle(.codync)
                    }
                }
                .padding(.horizontal, InterfaceMetrics.value(mac: 12, mobile: 18))
                .padding(.vertical, InterfaceMetrics.value(mac: 14, mobile: 20))
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))

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

                Text(draft.permission == "auto"
                    ? "Tool requests are approved automatically. Your agent's own settings (like Claude Code's permission rules) still apply."
                    : "You'll get an approval card and a notification whenever the agent asks.")
                    .font(.caption)
                    .foregroundStyle(Palette.tertiary)

                if let error {
                    Text(error).font(.footnote).foregroundStyle(Palette.danger)
                }
            }
            .padding(InterfaceMetrics.value(mac: 14, mobile: 20))
        }
        .font(InterfaceMetrics.body)
        #if os(macOS)
        .controlSize(.small)
        #endif
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
        .task { await model.refreshPlugins() }
        .codyncSheet(isPresented: $pickingFolder) {
            FolderPicker(start: draft.cwd.isEmpty ? model.hello?.home : draft.cwd) {
                draft.cwd = $0
                pickingFolder = false
            }
            #if os(macOS)
            .frame(width: 460, height: 520)
            #endif
        }
    }
}

private struct Field<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    init(_ label: String, @ViewBuilder content: () -> Content) {
        self.label = label
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(InterfaceMetrics.secondary).foregroundStyle(Palette.secondary).padding(.leading, 4)
            content
        }
    }
}

private struct OptionRow<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    init(_ label: String, @ViewBuilder content: () -> Content) {
        self.label = label
        self.content = content()
    }

    var body: some View {
        HStack {
            Text(label).foregroundStyle(Palette.text)
            Spacer(minLength: 12)
            content
        }
    }
}

/// A card of on/off switches for the connectors or skills installed on the computer.
private struct PluginToggles: View {
    let title: String
    let empty: String
    let items: [(id: String, name: String, detail: String)]
    @Binding var selection: [String]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(InterfaceMetrics.secondary).foregroundStyle(Palette.secondary).padding(.leading, 4)
            VStack(alignment: .leading, spacing: InterfaceMetrics.value(mac: 12, mobile: 16)) {
                if items.isEmpty {
                    Text(empty).font(InterfaceMetrics.secondary).foregroundStyle(Palette.secondary)
                }
                ForEach(items, id: \.id) { item in
                    Toggle(isOn: Binding(
                        get: { selection.contains(item.id) },
                        set: { on in
                            selection.removeAll { $0 == item.id }
                            if on { selection.append(item.id) }
                        }
                    )) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(item.name).foregroundStyle(Palette.text)
                            if !item.detail.isEmpty {
                                Text(item.detail).font(.caption).foregroundStyle(Palette.secondary).lineLimit(2)
                            }
                        }
                    }
                    .toggleStyle(.codync)
                }
            }
            .padding(InterfaceMetrics.value(mac: 12, mobile: 18))
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
    }
}

extension View {
    /// A rounded input box.
    func fieldBox() -> some View {
        textFieldStyle(.plain)
            .padding(.horizontal, InterfaceMetrics.value(mac: 10, mobile: 14))
            .padding(.vertical, InterfaceMetrics.value(mac: 8, mobile: 11))
            .background(Palette.background, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

extension BotDraft {
    var isValid: Bool { !name.trimmingCharacters(in: .whitespaces).isEmpty && !cwd.isEmpty }

    /// What the host should get: no stale command unless the agent is custom.
    var normalized: BotDraft {
        var d = self
        if d.backend != "custom" { d.command = nil }
        return d
    }
}

public extension BotStore {
    /// Picks an installed agent and the home folder for a new bot.
    func fillDefaults(_ draft: inout BotDraft) {
        if hello?.backends.first(where: { $0.id == draft.backend })?.available != true,
           let first = hello?.backends.first(where: \.available) {
            draft.backend = first.id
        }
        if draft.cwd.isEmpty, let home = hello?.home { draft.cwd = home }
    }

    /// Creates "New Bot" with sensible defaults and opens it (desktop compose flow).
    func createDefaultBot() async throws -> Bot {
        var draft = BotDraft(name: "New Bot")
        fillDefaults(&draft)
        let bot = try await save(draft)
        selection = bot.id
        return bot
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
                        .padding(.leading, InterfaceMetrics.value(mac: 10, mobile: 12))
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
            LazyVStack(alignment: .leading, spacing: InterfaceMetrics.value(mac: 14, mobile: 22)) {
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
            .font(InterfaceMetrics.body)
            .padding(InterfaceMetrics.value(mac: 14, mobile: 20))
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
        .task {
            do { listing = try await model.listDirs(path) } catch { self.error = error.localizedDescription }
        }
    }
}
