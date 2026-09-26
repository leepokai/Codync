import CodyncKit
import SwiftUI

/// Create a group chat, or change its name, what it's for and who's in it. Bots are
/// picked like recipients: chips on top, a search, and the bots not in it yet.
public struct GroupEditorView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    /// The group being edited; nil creates one.
    let groupId: String?
    @State private var name: String
    @State private var about: String
    @State private var members: [String]
    @State private var query = ""
    @State private var saving = false
    @State private var error: String?
    @State private var confirmDelete: Bot?

    public init(group: Bot? = nil, members: [String] = []) {
        groupId = group?.id
        _name = State(initialValue: group?.name ?? "")
        _about = State(initialValue: group?.description ?? "")
        _members = State(initialValue: group?.members ?? members)
    }

    private var picked: [Bot] { members.compactMap { model.bots[$0] } }
    /// Bots that can still join, filtered by the search.
    private var candidates: [Bot] {
        let q = query.trimmingCharacters(in: .whitespaces)
        return model.roster.filter { !$0.isGroup && !members.contains($0.id) && (q.isEmpty || $0.name.localizedCaseInsensitiveContains(q)) }
    }
    private var isNew: Bool { groupId == nil }
    private var canSave: Bool { !members.isEmpty && !saving }

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader(isNew ? "New group chat" : "Group chat") {
                if saving {
                    Spinner()
                } else {
                    IconButton(isNew ? "Create" : "Save", systemImage: "checkmark") { save() }
                        .disabled(!canSave)
                }
            }
            ScrollView {
                VStack(alignment: .leading, spacing: InterfaceMetrics.value(mac: 12, mobile: 18)) {
                    GroupAvatar(members: picked, size: 72)
                        .frame(maxWidth: .infinity)
                        .animation(Motion.layout, value: members)
                    Field("Bots · \(members.count)") {
                        VStack(alignment: .leading, spacing: 8) {
                            if !picked.isEmpty {
                                ScrollView(.horizontal, showsIndicators: false) {
                                    HStack(spacing: 6) {
                                        ForEach(picked) { bot in
                                            BotChip(bot: bot) { toggle(bot.id) }
                                                .transition(.scale(scale: 0.85).combined(with: .opacity))
                                        }
                                    }
                                }
                            }
                            TextField(picked.isEmpty ? "Search bots" : "Add another bot", text: $query)
                                .fieldBox()
                                .onSubmit { if let first = candidates.first { toggle(first.id) } }
                            VStack(spacing: 2) {
                                ForEach(candidates) { bot in candidateRow(bot) }
                            }
                        }
                        .animation(Motion.layout, value: members)
                        Text("Everyone answers in turn unless you @mention someone. Each bot works in its own folder with its own tools.")
                            .font(.caption)
                            .foregroundStyle(Palette.tertiary)
                    }
                    Field("Name") {
                        TextField(defaultName, text: $name).fieldBox()
                    }
                    Field("About") {
                        TextField("What this group works on (optional)", text: $about, axis: .vertical)
                            .lineLimit(2...5)
                            .fieldBox()
                    }
                    if let error {
                        Text(error).font(.footnote).foregroundStyle(Palette.danger).transition(.opacity)
                    }
                    if let group = groupId.flatMap({ model.bots[$0] }) {
                        Button("Delete group chat", role: .destructive) { confirmDelete = group }
                            .buttonStyle(.secondary)
                            .padding(.top, 8)
                    }
                }
                .padding(InterfaceMetrics.value(mac: 16, mobile: 20))
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .background(Palette.background)
        .deleteBotConfirmation($confirmDelete) { dismiss() }
    }

    /// "Alice, Bob" when no name is typed.
    private var defaultName: String {
        let names = picked.map(\.name)
        return names.isEmpty ? "Name" : names.joined(separator: ", ")
    }

    private func toggle(_ id: String) {
        withAnimation(Motion.layout) {
            if members.contains(id) { members.removeAll { $0 == id } } else { members.append(id) }
        }
        query = ""
    }

    private func candidateRow(_ bot: Bot) -> some View {
        Button { toggle(bot.id) } label: {
            HStack(spacing: 12) {
                CharacterAvatar(bot: bot, size: InterfaceMetrics.value(mac: 26, mobile: 34), animated: false)
                VStack(alignment: .leading, spacing: 1) {
                    Text(bot.name).foregroundStyle(Palette.text).lineLimit(1)
                    Text(bot.folderName).font(.caption).foregroundStyle(Palette.tertiary).lineLimit(1)
                }
                Spacer()
                Image(systemName: "plus.circle")
                    .font(.system(size: 20))
                    .foregroundStyle(Palette.tertiary)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScale())
        .accessibilityLabel("Add \(bot.name)")
    }

    private func save() {
        saving = true
        error = nil
        let typed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let finalName = typed.isEmpty ? defaultName : typed
        let about = about.trimmingCharacters(in: .whitespacesAndNewlines)
        Task {
            do {
                if let groupId {
                    var draft = GroupDraft(name: finalName, description: about, members: members)
                    draft.id = groupId
                    draft.pinned = model.bots[groupId]?.pinned
                    try await model.updateGroup(draft)
                } else {
                    _ = try await model.createGroup(name: finalName, description: about, members: members)
                }
                dismiss()
            } catch {
                withAnimation(Motion.fade) { self.error = error.localizedDescription }
            }
            saving = false
        }
    }
}

/// A picked bot (To: field, group editor); its x takes it back out.
struct BotChip: View {
    let bot: Bot
    let remove: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            CharacterAvatar(bot: bot, size: 20, animated: false)
            Text(bot.name)
                .font(.body)
                .foregroundStyle(Palette.text)
                .lineLimit(1)
                .frame(maxWidth: 220, alignment: .leading)
                .fixedSize(horizontal: true, vertical: false)
            Button(action: remove) {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(Palette.secondary)
                    .frame(width: 18, height: 18)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Remove \(bot.name)")
            .help("Remove \(bot.name)")
        }
        .padding(.leading, 8)
        .padding(.trailing, 6)
        .padding(.vertical, 5)
        .background(Palette.bubbleAgent, in: Capsule())
    }
}

