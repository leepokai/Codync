import CodyncKit
import SwiftUI

/// Bot settings' Workspace on a host with projects (`hello.projects`), each a row of the card: the folders the bot
/// sends work to (the first its default), when its threads get a worktree there, and its own folder, where it lives.
struct BotProjects: View {
    @Binding var draft: BotDraft
    let bot: Bot?
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var picking = false
    /// The project whose shared folders are being edited.
    @State private var sharing: String?

    private var projects: [String] { draft.projects ?? [] }

    var body: some View {
        ValueRow("Projects", detail: "Threads start in the default project. The bot can work elsewhere when you ask.") {
            IconButton("Add project", systemImage: "plus") { picking = true }
                .padding(.vertical, -8)
        }
        .codyncSheet(isPresented: $picking) {
            FolderPicker(start: projects.first ?? model.hello?.home) { path in
                animate { draft.addProject(path) }
                picking = false
            }
        }
        .codyncSheet(isPresented: Binding(get: { sharing != nil }, set: { if !$0 { sharing = nil } })) {
            if let sharing { SharedFoldersSheet(project: sharing, draft: $draft) }
        }
        ForEach(projects, id: \.self) { row($0) }
        // Only once one of its projects is a git repository (as the host last said): elsewhere no worktree is made.
        if let bot, projects.contains(where: bot.isGitProject) {
            let mode = draft.worktreeMode ?? .sideBySide
            ValueRow("Worktree mode", detail: mode.line) {
                ChoicePicker(selection: Binding(get: { mode }, set: { next in animate { draft.worktreeMode = next } }),
                             options: WorktreeMode.allCases.map { (id: $0, label: $0.title) })
            }
        }
        // A new bot has no folder until it's created.
        if let home = bot?.cwd, !home.isEmpty { HomeFolderRow(home: home) }
    }

    /// A project: its name over its full path, "Default" on the first, Make default on the others, and Remove.
    private func row(_ path: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: bot?.isGitProject(path) == true ? "arrow.triangle.branch" : "folder").foregroundStyle(Palette.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: path.folderName).foregroundStyle(Palette.text).lineLimit(1)
                Text(verbatim: path).font(.caption).foregroundStyle(Palette.secondary).lineLimit(1).truncationMode(.middle)
            }
            Spacer(minLength: 4)
            // Only where a worktree is made and the host links them in.
            if model.hello?.worktreeFill == true, bot?.isGitProject(path) == true {
                IconButton("Shared folders", systemImage: "link") { sharing = path }
                    .padding(.vertical, -8)
            }
            if path == projects.first {
                Text(L10n.tr("Default")).font(.caption).foregroundStyle(Palette.secondary)
            } else {
                IconButton("Make default", systemImage: "star") { animate { draft.makeDefaultProject(path) } }
                    .padding(.vertical, -8)
            }
            IconButton("Remove project", systemImage: "minus.circle") { animate { draft.removeProject(path) } }
                .padding(.vertical, -8)
        }
        .transition(.opacity)
    }

    private func animate(_ action: () -> Void) { withAnimation(Motion.reduced(Motion.layout, reduceMotion), action) }
}

/// The bot's own folder, where its chat, routines and files live: shown as it is, with a button that copies it.
private struct HomeFolderRow: View {
    let home: String
    @State private var copied = false

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(L10n.tr("Bot's folder")).foregroundStyle(Palette.text)
                Text(L10n.tr("Its chat, routines and files live here.")).font(.caption).foregroundStyle(Palette.secondary)
                Text(verbatim: copied ? L10n.tr("Path copied") : home).font(.caption).foregroundStyle(Palette.secondary)
                    .lineLimit(1).truncationMode(.middle)
            }
            Spacer(minLength: 4)
            IconButton("Copy path", systemImage: "square.on.square") {
                Pasteboard.copy(home)
                copied = true
            }
            .padding(.vertical, -8)
        }
        .task(id: copied) {
            guard copied, (try? await Task.sleep(for: .seconds(1.5))) != nil else { return }
            copied = false
        }
    }
}

/// A git project's shared folders: the names (`node_modules`) linked into each of its new worktrees, added and removed
/// in the draft, saved with the bot.
private struct SharedFoldersSheet: View {
    let project: String
    @Binding var draft: BotDraft
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var name = ""
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader { Text(verbatim: "\(L10n.tr("Shared folders")) · \(project.folderName)") }
            CardForm {
                CardSection(footer: "Linked into each new worktree, so installed packages don't have to be installed again.") {
                    ForEach(draft.sharedFolders(of: project), id: \.self) { folder in
                        HStack(spacing: 12) {
                            Image(systemName: "folder").foregroundStyle(Palette.secondary)
                            Text(verbatim: folder).foregroundStyle(Palette.text).lineLimit(1).truncationMode(.middle)
                            Spacer(minLength: 4)
                            IconButton("Remove folder", systemImage: "minus.circle") {
                                animate { draft.removeSharedFolder(folder, from: project) }
                            }
                            .padding(.vertical, -8)
                        }
                        .transition(.opacity)
                    }
                    HStack(spacing: 12) {
                        TextField(L10n.tr("Folder name"), text: $name)
                            .plainTextInput().focused($focused).submitLabel(.done).onSubmit(add)
                        IconButton("Add folder", systemImage: "plus", action: add)
                            .disabled(BotDraft.sharedFolderName(name) == nil)
                            .padding(.vertical, -8)
                    }
                }
            }
            .textFieldStyle(.plain)
        }
        .background(Palette.background)
        .presentationDetents([.medium, .large])
    }

    private func add() {
        guard BotDraft.sharedFolderName(name) != nil else { return }
        animate { draft.addSharedFolder(name, to: project) }
        name = ""
        focused = true
    }

    private func animate(_ action: () -> Void) { withAnimation(Motion.reduced(Motion.layout, reduceMotion), action) }
}
