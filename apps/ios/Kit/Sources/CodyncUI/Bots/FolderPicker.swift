import CodyncKit
import SwiftUI

/// Chooses a folder on the bot's computer (never another's), the system way: a sheet with its own navigation stack,
/// the computer and its recent workspaces first (searchable, the one picked checked), and Browse other folders pushing
/// one page per folder. Cancel and Use this folder sit in the navigation bar.
struct FolderPicker: View {
    let start: String?
    let onPick: (String) -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var stack: [String] = []
    @State private var recents = WorkspacePickerState()
    @State private var selected = ""
    @State private var filter = ""
    @State private var saving = false
    @State private var saveError: String?
    /// Opens tall, so Browse other folders shows under a long history; it can be pulled down to half.
    @State private var detent = PresentationDetent.large

    private var online: Bool { model.connection == .online }
    private var history: [RecentWorkspace] { recents.listing?.recent ?? [] }
    private var shown: [RecentWorkspace] { history.filter { filter.isEmpty || $0.path.localizedCaseInsensitiveContains(filter) } }

    var body: some View {
        NavigationStack(path: $stack) {
            recentPage
                .navigationDestination(for: String.self) { path in
                    FolderPage(path: path, stack: $stack, online: online, saving: saving, saveError: saveError, use: use)
                }
        }
        .presentationDetents([.medium, .large], selection: $detent)
        .task(id: model.connection) { await recents.load(nil, model: model) }
    }

    private var recentPage: some View {
        List {
            Section {
                ComputerRow(online: online)
            } header: {
                Text(L10n.tr("Your bot will work in this folder.")).textCase(nil)
            }
            if recents.busy {
                Section { LoadingRow() }
            } else if !history.isEmpty {
                Section(L10n.tr("Recent workspaces")) {
                    ForEach(shown) { row($0) }
                    if shown.isEmpty { NoteRow("No matching folders.") }
                }
            }
            if let error = saveError ?? recents.error { Section { ErrorRow(error) } }
            // With no history this is the first thing under the computer.
            Section {
                NavigationLink(value: start ?? "") { Label(L10n.tr("Browse other folders"), systemImage: "folder") }
            }
        }
        .disabled(saving)
        .searchable(text: $filter, prompt: L10n.tr("Search recent workspaces"))
        .searchPresentationToolbarBehavior(.avoidHidingContent)
        .navigationTitle(L10n.tr("Choose workspace"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button { dismiss() } label: { Image(systemName: "xmark") }.accessibilityLabel(L10n.tr("Cancel"))
            }
            ToolbarItem(placement: .confirmationAction) {
                UseFolderButton(saving: saving, enabled: !selected.isEmpty && online && !recents.busy) { use(selected, recents.listing) }
            }
        }
    }

    private func row(_ item: RecentWorkspace) -> some View {
        Button { selected = item.path } label: {
            HStack(spacing: 12) {
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(item.path.folderName).lineLimit(1)
                        Text(item.path).font(.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                    }
                } icon: { Image(systemName: "folder") }
                Spacer(minLength: 4)
                if selected == item.path { Image(systemName: "checkmark").fontWeight(.semibold).foregroundStyle(.tint) }
            }
            .contentShape(Rectangle())
        }
        .foregroundStyle(.primary)
        .accessibilityAddTraits(selected == item.path ? .isSelected : [])
    }

    /// Checks the folder on the computer (and records it there as recent), then hands back the path it resolved.
    private func use(_ path: String, _ listing: DirListing?) {
        guard !path.isEmpty, !saving else { return }
        saving = true
        saveError = nil
        Task {
            defer { saving = false }
            do {
                // Absence of `recent` means an older host: validate with its existing browsing API.
                let chosen = listing?.recent != nil ? try await model.selectWorkspace(path) : try await model.listDirs(path).path
                onPick(chosen)
            } catch {
                saveError = error.localizedDescription
            }
        }
    }
}

/// One folder: its subfolders, each pushing its own page, filtered by the search field. The title's menu goes to the
/// parent or an ancestor, or opens a field for a pasted path; Use this folder takes the folder shown.
private struct FolderPage: View {
    /// Empty for the computer's home.
    let path: String
    @Binding var stack: [String]
    let online: Bool
    let saving: Bool
    let saveError: String?
    let use: (String, DirListing?) -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var folder = WorkspacePickerState()
    @State private var filter = ""
    @State private var typing = false
    @State private var typed = ""
    @FocusState private var focused: Bool

    private var dirs: [DirItem] { (folder.listing?.dirs ?? []).filter { filter.isEmpty || $0.name.localizedCaseInsensitiveContains(filter) } }
    private var title: String {
        let name = (folder.listing?.path ?? path).folderName
        return name.isEmpty ? L10n.tr("Folders") : name
    }

    var body: some View {
        List {
            if typing {
                Section {
                    TextField(L10n.tr("Paste an absolute path"), text: $typed)
                        .plainTextInput().focused($focused).submitLabel(.go).onSubmit(go)
                }
            }
            if folder.busy {
                Section { LoadingRow() }
            } else if let listing = folder.listing {
                Section {
                    ForEach(dirs) { dir in
                        NavigationLink(value: dir.path) { Label(dir.name, systemImage: dir.isGit ? "arrow.triangle.branch" : "folder") }
                    }
                    if dirs.isEmpty { NoteRow(filter.isEmpty ? "No subfolders. You can use this folder." : "No matching folders.") }
                } header: {
                    Text(listing.path).textCase(nil).lineLimit(1).truncationMode(.middle)
                }
            }
            if let error = saveError ?? folder.error { Section { ErrorRow(error) } }
        }
        .disabled(saving)
        .searchable(text: $filter, prompt: L10n.tr("Filter this folder"))
        .searchPresentationToolbarBehavior(.avoidHidingContent)
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarTitleMenu { titleMenu }
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                UseFolderButton(saving: saving, enabled: folder.listing != nil && online && !folder.busy) {
                    if let listing = folder.listing { use(listing.path, listing) }
                }
            }
        }
        .task(id: model.connection) { await folder.load(path.isEmpty ? nil : path, model: model) }
    }

    @ViewBuilder private var titleMenu: some View {
        Button(L10n.tr("Enter folder path"), systemImage: "pencil") {
            typed = folder.listing?.path ?? path
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { typing = true }
            focused = true
        }
        if let parent = folder.listing?.parent {
            Button(L10n.tr("Parent folder"), systemImage: "arrow.up") { open(parent) }
        }
        if let ancestors = folder.listing?.ancestors?.dropLast(), !ancestors.isEmpty {
            Section {
                ForEach(ancestors.reversed()) { item in
                    Button(item.name.isEmpty ? item.path : item.name, systemImage: "folder") { open(item.path) }
                }
            }
        }
    }

    /// A folder already in the stack goes back to it; any other is pushed.
    private func open(_ next: String) {
        if let index = stack.firstIndex(of: next) { stack = Array(stack.prefix(through: index)) } else { stack.append(next) }
    }

    private func go() {
        let next = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !next.isEmpty else { return }
        typing = false
        open(next)
    }
}

/// The navigation bar's answer, in prominent glass as the sheet's confirming action.
private struct UseFolderButton: View {
    let saving: Bool
    let enabled: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            if saving { ProgressView() } else { Text(L10n.tr("Use this folder")) }
        }
        .buttonStyle(.glassProminent)
        .disabled(!enabled || saving)
    }
}

/// Which computer the folders are on, and whether it is connected.
private struct ComputerRow: View {
    let online: Bool
    @Environment(BotStore.self) private var model

    var body: some View {
        LabeledContent {
            Text(L10n.tr(online ? "Connected" : "Disconnected"))
        } label: {
            Label(model.hello?.name ?? L10n.tr("Computer"), systemImage: "desktopcomputer").lineLimit(1)
        }
    }
}

private struct LoadingRow: View {
    var body: some View { ProgressView().frame(maxWidth: .infinity) }
}

private struct NoteRow: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View { Text(L10n.tr(text)).foregroundStyle(.secondary) }
}

private struct ErrorRow: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View { Text(text).foregroundStyle(.red) }
}
