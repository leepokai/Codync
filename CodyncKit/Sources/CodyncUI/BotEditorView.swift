import CodyncKit
import SwiftUI

public struct EditorRequest: Identifiable {
    public let id = UUID()
    public var draft: BotDraft

    public init(_ draft: BotDraft) { self.draft = draft }
}

/// Create or edit a bot: who it is, which agent, which project, how much it may do alone.
public struct BotEditorView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State var draft: BotDraft

    public init(draft: BotDraft) { _draft = State(initialValue: draft) }
    @State private var saving = false
    @State private var error: String?
    @State private var pickingFolder = false

    private var isNew: Bool { draft.id == nil }

    public var body: some View {
        Form {
            Section {
                VStack(spacing: 14) {
                    CharacterAvatar(shape: draft.avatarShape, color: draft.avatarColor, size: 84, mood: .working)
                    TextField("Name", text: $draft.name)
                        .font(.title3.weight(.semibold))
                        .multilineTextAlignment(.center)
                    AvatarPicker(shape: $draft.avatarShape, color: $draft.avatarColor)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 6)
            }

            Section {
                TextField("e.g. Reviews PRs. Never pushes without asking. Answers in Traditional Chinese.", text: $draft.description, axis: .vertical)
                    .lineLimit(3...8)
            } header: {
                Text("Standing instructions")
            } footer: {
                Text("Rules that always apply. Put task-specific requests in the chat instead.")
            }

            Section("Agent") {
                Picker("Runs on", selection: $draft.backend) {
                    ForEach(model.hello?.backends ?? []) { b in
                        Text(b.available ? b.name : "\(b.name) (not installed)").tag(b.id)
                    }
                    Text("Custom command").tag("custom")
                }
                if let b = model.hello?.backends.first(where: { $0.id == draft.backend }), !b.available {
                    Text(b.installHint).font(.footnote).foregroundStyle(Palette.warning)
                }
                if draft.backend == "custom" {
                    TextField("ACP command, e.g. my-agent --acp", text: Binding(get: { draft.command ?? "" }, set: { draft.command = $0.isEmpty ? nil : $0 }))
                        .font(.callout.monospaced())
                        .plainTextInput()
                        .autocorrectionDisabled()
                }
                TextField("Model (optional)", text: Binding(get: { draft.model ?? "" }, set: { draft.model = $0.isEmpty ? nil : $0 }))
                    .plainTextInput()
                    .autocorrectionDisabled()
            }

            Section {
                Button {
                    pickingFolder = true
                } label: {
                    LabeledContent("Project folder") {
                        Text(draft.cwd.isEmpty ? "Choose…" : (draft.cwd as NSString).lastPathComponent)
                            .foregroundStyle(draft.cwd.isEmpty ? Palette.accent : Palette.secondary)
                    }
                    .foregroundStyle(Palette.text)
                }
            } footer: {
                if !draft.cwd.isEmpty {
                    Text(draft.cwd).font(.caption.monospaced())
                }
            }

            Section {
                Picker("Permissions", selection: $draft.permission) {
                    Text("Ask me").tag("ask")
                    Text("Approve automatically").tag("auto")
                }
                Toggle("Notifications", isOn: Binding(get: { draft.notify ?? true }, set: { draft.notify = $0 }))
            } footer: {
                Text(draft.permission == "auto"
                    ? "Tool requests are approved once, automatically. Your agent's own settings (like Claude Code's permission rules) still apply."
                    : "You'll get an approval card and a notification whenever the agent asks.")
            }

            if let error {
                Section { Text(error).foregroundStyle(Palette.danger) }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Palette.background)
        .navigationTitle(isNew ? "New bot" : "Edit bot")
        .inlineNavigationTitle()
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            ToolbarItem(placement: .confirmationAction) {
                if saving {
                    ProgressView()
                } else {
                    Button(isNew ? "Create" : "Save") { save() }
                        .disabled(draft.name.trimmingCharacters(in: .whitespaces).isEmpty || draft.cwd.isEmpty)
                }
            }
        }
        .sheet(isPresented: $pickingFolder) {
            NavigationStack {
                FolderPicker(path: draft.cwd.isEmpty ? model.hello?.home : draft.cwd) {
                    draft.cwd = $0
                    pickingFolder = false
                }
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { pickingFolder = false } }
                }
            }
        }
        .onAppear {
            if isNew, draft.backend == "claude", model.hello?.backends.first(where: { $0.id == "claude" })?.available == false,
               let first = model.hello?.backends.first(where: \.available) {
                draft.backend = first.id
            }
        }
    }

    private func save() {
        saving = true
        error = nil
        if draft.backend != "custom" { draft.command = nil }
        Task {
            do {
                let bot = try await model.save(draft)
                dismiss()
                if isNew { model.selection = bot.id }
            } catch {
                self.error = error.localizedDescription
            }
            saving = false
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
