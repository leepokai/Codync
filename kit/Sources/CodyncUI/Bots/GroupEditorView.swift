import CodyncKit
import SwiftUI

/// Create a group chat, or rename one and change who's in it (up to six bots).
public struct GroupEditorView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    /// The group being edited; nil creates one.
    let groupId: String?
    @State private var name: String
    @State private var members: [String]
    @State private var saving = false
    @State private var error: String?
    @State private var confirmDelete: Bot?

    static let maxMembers = 6

    public init(group: Bot? = nil, members: [String] = []) {
        groupId = group?.id
        _name = State(initialValue: group?.name ?? "")
        _members = State(initialValue: group?.members ?? members)
    }

    private var candidates: [Bot] { model.roster.filter { !$0.isGroup } + model.hiddenBots.filter { members.contains($0.id) } }
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
                    GroupAvatar(members: members.compactMap { model.bots[$0] }, size: 72)
                        .frame(maxWidth: .infinity)
                        .animation(Motion.layout, value: members)
                    Field("Name") {
                        TextField(defaultName, text: $name).fieldBox()
                    }
                    Field("Bots · \(members.count) of \(Self.maxMembers)") {
                        VStack(spacing: 2) {
                            ForEach(candidates) { bot in memberRow(bot) }
                        }
                        Text("Everyone answers in turn unless you @mention someone. Each bot works in its own folder with its own tools.")
                            .font(.caption)
                            .foregroundStyle(Palette.tertiary)
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
        }
        .background(Palette.background)
        .deleteBotConfirmation($confirmDelete) { dismiss() }
    }

    /// "Alice, Bob" when no name is typed.
    private var defaultName: String {
        let names = members.compactMap { model.bots[$0]?.name }
        return names.isEmpty ? "Name" : names.joined(separator: ", ")
    }

    private func memberRow(_ bot: Bot) -> some View {
        let on = members.contains(bot.id)
        return Button {
            withAnimation(Motion.layout) {
                if on { members.removeAll { $0 == bot.id } } else if members.count < Self.maxMembers { members.append(bot.id) }
            }
        } label: {
            HStack(spacing: 12) {
                CharacterAvatar(bot: bot, size: InterfaceMetrics.value(mac: 26, mobile: 34), animated: false)
                VStack(alignment: .leading, spacing: 1) {
                    Text(bot.name).foregroundStyle(Palette.text).lineLimit(1)
                    Text(bot.folderName).font(.caption).foregroundStyle(Palette.tertiary).lineLimit(1)
                }
                Spacer()
                Image(systemName: on ? "checkmark.circle.fill" : "circle")
                    .font(.system(size: 20))
                    .foregroundStyle(on ? Palette.accent : Palette.tertiary)
                    .contentTransition(.symbolEffect(.replace))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScale())
        .disabled(!on && members.count >= Self.maxMembers)
        .accessibilityAddTraits(on ? .isSelected : [])
    }

    private func save() {
        saving = true
        error = nil
        let typed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let finalName = typed.isEmpty ? defaultName : typed
        Task {
            do {
                if let groupId {
                    var draft = GroupDraft(name: finalName, members: members)
                    draft.id = groupId
                    try await model.updateGroup(draft)
                } else {
                    _ = try await model.createGroup(name: finalName, members: members)
                }
                dismiss()
            } catch {
                withAnimation(Motion.fade) { self.error = error.localizedDescription }
            }
            saving = false
        }
    }
}
