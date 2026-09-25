import CodyncKit
import SwiftUI

public struct EditorRequest: Identifiable {
    public let id = UUID()
    public var draft: BotDraft

    public init(_ draft: BotDraft) { self.draft = draft }
}

/// Create or edit a bot in a sheet (iPhone): the settings form plus Cancel / Save.
public struct BotEditorView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State var draft: BotDraft

    public init(draft: BotDraft) { _draft = State(initialValue: draft) }
    @State private var saving = false
    @State private var error: String?

    private var isNew: Bool { draft.id == nil }

    public var body: some View {
        BotSettingsForm(draft: $draft, error: error)
            .navigationTitle(isNew ? "New bot" : "Settings")
            .inlineNavigationTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", systemImage: "xmark") { dismiss() }.labelStyle(.iconOnly).help("Cancel")
                }
                ToolbarItem(placement: .confirmationAction) {
                    if saving {
                        ProgressView()
                    } else {
                        Button(isNew ? "Create" : "Save", systemImage: "checkmark") { save() }
                            .labelStyle(.iconOnly)
                            .help(isNew ? "Create" : "Save")
                            .disabled(!draft.isValid)
                    }
                }
            }
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

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                VStack(spacing: 16) {
                    CharacterAvatar(shape: draft.avatarShape, color: draft.avatarColor, size: 96)
                        .padding(18)
                        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
                    AvatarPicker(shape: $draft.avatarShape, color: $draft.avatarColor)
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

                VStack(spacing: 0) {
                    OptionRow("Agent") {
                        Picker("Agent", selection: $draft.backend) {
                            ForEach(model.hello?.backends ?? []) { b in
                                Text(b.available ? b.name : "\(b.name) (not installed)").tag(b.id)
                            }
                            Text("Custom command").tag("custom")
                        }
                        .labelsHidden()
                        .fixedSize()
                    }
                    if let b = model.hello?.backends.first(where: { $0.id == draft.backend }), !b.available {
                        Text(b.installHint).font(.caption).foregroundStyle(Palette.warning)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.bottom, 10)
                    }
                    if draft.backend == "custom" {
                        TextField("ACP command, e.g. my-agent --acp", text: Binding(get: { draft.command ?? "" }, set: { draft.command = $0.isEmpty ? nil : $0 }))
                            .font(.callout.monospaced())
                            .plainTextInput()
                            .fieldBox()
                            .padding(.bottom, 10)
                    }
                    Divider().overlay(Palette.border)
                    OptionRow("Model") {
                        TextField("Default", text: Binding(get: { draft.model ?? "" }, set: { draft.model = $0.isEmpty ? nil : $0 }))
                            .plainTextInput()
                            .multilineTextAlignment(.trailing)
                            .textFieldStyle(.plain)
                            .frame(maxWidth: 200)
                    }
                    Divider().overlay(Palette.border)
                    OptionRow("Project folder") {
                        Button(draft.cwd.isEmpty ? "Choose…" : (draft.cwd as NSString).lastPathComponent) { pickingFolder = true }
                            .buttonStyle(.plain)
                            .foregroundStyle(draft.cwd.isEmpty ? Palette.text : Palette.secondary)
                            .help(draft.cwd)
                    }
                    Divider().overlay(Palette.border)
                    OptionRow("Permissions") {
                        Picker("Permissions", selection: $draft.permission) {
                            Text("Ask me").tag("ask")
                            Text("Approve automatically").tag("auto")
                        }
                        .labelsHidden()
                        .fixedSize()
                    }
                    Divider().overlay(Palette.border)
                    Toggle(isOn: Binding(get: { draft.notify ?? true }, set: { draft.notify = $0 })) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Notifications").foregroundStyle(Palette.text)
                            Text("When it finishes or needs you").font(.caption).foregroundStyle(Palette.secondary)
                        }
                    }
                    .toggleStyle(.switch)
                    .padding(.vertical, 12)
                }
                .padding(.horizontal, 16)
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 16, style: .continuous))

                Text(draft.permission == "auto"
                    ? "Tool requests are approved automatically. Your agent's own settings (like Claude Code's permission rules) still apply."
                    : "You'll get an approval card and a notification whenever the agent asks.")
                    .font(.caption)
                    .foregroundStyle(Palette.tertiary)

                if let error {
                    Text(error).font(.footnote).foregroundStyle(Palette.danger)
                }
            }
            .padding(20)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
        .sheet(isPresented: $pickingFolder) {
            NavigationStack {
                FolderPicker(path: draft.cwd.isEmpty ? model.hello?.home : draft.cwd) {
                    draft.cwd = $0
                    pickingFolder = false
                }
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel", systemImage: "xmark") { pickingFolder = false }.labelStyle(.iconOnly).help("Cancel")
                    }
                }
            }
            #if os(macOS)
            .frame(minWidth: 460, minHeight: 520)
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
            Text(label).font(.subheadline).foregroundStyle(Palette.secondary).padding(.leading, 4)
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
        .frame(minHeight: 46)
    }
}

extension View {
    /// A bordered, rounded input box.
    func fieldBox() -> some View {
        textFieldStyle(.plain)
            .padding(.horizontal, 14)
            .padding(.vertical, 11)
            .background(Palette.background, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Palette.border))
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
                            .background(Circle().stroke(s == shape ? Palette.accent : .clear, lineWidth: 2))
                            .onTapGesture { shape = s }
                            .accessibilityLabel("\(s) shape")
                            .accessibilityAddTraits(s == shape ? .isSelected : [])
                    }
                }
                .padding(.horizontal, 4)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 10) {
                    ForEach(AvatarPalette.colors) { c in
                        Circle()
                            .fill(c.color)
                            .frame(width: 26, height: 26)
                            .padding(3)
                            .background(Circle().stroke(c.id == color ? Palette.accent : .clear, lineWidth: 2))
                            .onTapGesture { color = c.id }
                            .accessibilityLabel(c.label)
                            .accessibilityAddTraits(c.id == color ? .isSelected : [])
                    }
                }
                .padding(.horizontal, 4)
            }
        }
    }
}

/// Browses folders on the host.
struct FolderPicker: View {
    let path: String?
    let onPick: (String) -> Void
    @Environment(BotStore.self) private var model
    @State private var listing: DirListing?
    @State private var error: String?
    @State private var filter = ""

    var body: some View {
        List {
            if let listing {
                Section {
                    Button {
                        onPick(listing.path)
                    } label: {
                        Label("Use “\((listing.path as NSString).lastPathComponent)”", systemImage: "checkmark.circle.fill")
                            .font(.headline)
                    }
                } footer: {
                    Text(listing.path).font(.caption.monospaced())
                }
                Section {
                    ForEach(listing.dirs.filter { filter.isEmpty || $0.name.localizedCaseInsensitiveContains(filter) }) { dir in
                        NavigationLink {
                            FolderPicker(path: dir.path, onPick: onPick)
                        } label: {
                            Label {
                                Text(dir.name)
                            } icon: {
                                Image(systemName: dir.isGit ? "arrow.triangle.branch" : "folder")
                                    .foregroundStyle(dir.isGit ? Palette.accent : Palette.secondary)
                            }
                        }
                    }
                }
            } else if let error {
                Text(error).foregroundStyle(Palette.danger)
            } else {
                ProgressView()
            }
        }
        .searchable(text: $filter)
        .navigationTitle(listing.map { ($0.path as NSString).lastPathComponent } ?? "Folders")
        .inlineNavigationTitle()
        .task {
            do { listing = try await model.listDirs(path) } catch { self.error = error.localizedDescription }
        }
    }
}
