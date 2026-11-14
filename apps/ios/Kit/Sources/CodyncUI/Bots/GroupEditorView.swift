import CodyncKit
import SwiftUI

/// Change a group chat's name, what it's for and who's in it. Bots are picked like
/// recipients: chips on top, a search, and the bots not in it yet.
public struct GroupEditorView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    let groupId: String
    @State private var name: String
    @State private var about: String
    @State private var members: [String]
    @State private var query = ""
    @State private var adding = false
    @State private var saving = false
    @State private var error: String?
    @State private var confirmDelete: Bot?

    public init(group: Bot) {
        groupId = group.id
        _name = State(initialValue: group.name)
        _about = State(initialValue: group.description)
        _members = State(initialValue: group.members)
    }

    private var picked: [Bot] { members.compactMap { model.bots[$0] } }
    /// Bots that can still join, filtered by the search.
    private var candidates: [Bot] {
        let q = query.trimmingCharacters(in: .whitespaces)
        return model.roster.filter { !$0.isGroup && !members.contains($0.id) && (q.isEmpty || $0.name.localizedCaseInsensitiveContains(q)) }
    }
    private var canSave: Bool { !members.isEmpty && !saving }

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Group chat") {
                if saving {
                    Spinner()
                } else {
                    IconButton("Save", systemImage: "checkmark") { save() }
                        .disabled(!canSave)
                }
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    VStack(spacing: 10) {
                        GroupAvatar(members: picked, size: 84)
                            .animation(Motion.layout, value: members)
                        TextField(defaultName, text: $name)
                            .textFieldStyle(.plain)
                            .font(.title2.weight(.semibold))
                            .foregroundStyle(Palette.text)
                            .multilineTextAlignment(.center)
                            .padding(.top, 6)
                        TextField("What this group works on", text: $about, axis: .vertical)
                            .textFieldStyle(.plain)
                            .lineLimit(1...4)
                            .font(.subheadline)
                            .foregroundStyle(Palette.secondary)
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity)
                    VStack(alignment: .leading, spacing: 0) {
                        Text(L10n.tr("Members"))
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(Palette.secondary)
                            .padding(.bottom, 4)
                        ForEach(picked) { bot in
                            memberRow(bot) {
                                IconButton("Remove \(bot.name)", systemImage: "xmark") { toggle(bot.id) }
                            }
                            .transition(.opacity)
                        }
                        if adding {
                            TextField("Search bots", text: $query)
                                .fieldBox(fill: Palette.surface)
                                .onSubmit { if let first = candidates.first { toggle(first.id) } }
                                .padding(.vertical, 6)
                            ForEach(candidates) { bot in
                                Button { toggle(bot.id) } label: {
                                    memberRow(bot, dimmed: true) {
                                        Image(systemName: "plus.circle").font(.system(size: 18)).foregroundStyle(Palette.secondary)
                                    }
                                }
                                .buttonStyle(PressScale())
                                .accessibilityLabel("Add \(bot.name)")
                            }
                        } else if !candidates.isEmpty {
                            Button { withAnimation(Motion.layout) { adding = true } } label: {
                                HStack(spacing: 14) {
                                    Image(systemName: "plus").font(.system(size: 16)).foregroundStyle(Palette.secondary).frame(width: 32)
                                    Text(L10n.tr("Add Member")).foregroundStyle(Palette.secondary)
                                    Spacer(minLength: 0)
                                }
                                .frame(minHeight: 48)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(PressScale())
                        }
                        Text(L10n.tr("Everyone answers in turn unless you @mention someone."))
                            .font(.caption)
                            .foregroundStyle(Palette.tertiary)
                            .padding(.top, 10)
                    }
                    .animation(Motion.layout, value: members)
                    if let error {
                        Text(L10n.tr(error)).font(.footnote).foregroundStyle(Palette.danger).transition(.opacity)
                    }
                    if let group = model.bots[groupId] {
                        Button(L10n.tr("Delete group chat"), role: .destructive) { confirmDelete = group }
                            .foregroundStyle(Palette.danger)
                            .buttonStyle(PressScale())
                    }
                }
                .padding(.horizontal, 20)
                .padding(.top, 8)
                .padding(.bottom, 24)
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

    /// A member or candidate: avatar, name, and a trailing control (Grok Bot's member list).
    private func memberRow(_ bot: Bot, dimmed: Bool = false, @ViewBuilder trailing: () -> some View) -> some View {
        HStack(spacing: 14) {
            CharacterAvatar(bot: bot, size: 32, animated: false)
            Text(bot.name).foregroundStyle(dimmed ? Palette.secondary : Palette.text).lineLimit(1)
            Spacer(minLength: 0)
            trailing()
        }
        .frame(minHeight: 48)
        .contentShape(Rectangle())
    }

    private func save() {
        saving = true
        error = nil
        let typed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let finalName = typed.isEmpty ? defaultName : typed
        let about = about.trimmingCharacters(in: .whitespacesAndNewlines)
        Task {
            do {
                var draft = GroupDraft(name: finalName, description: about, members: members)
                draft.id = groupId
                draft.pinned = model.bots[groupId]?.pinned
                try await model.updateGroup(draft)
                dismiss()
            } catch {
                withAnimation(Motion.fade) { self.error = error.localizedDescription }
            }
            saving = false
        }
    }
}
