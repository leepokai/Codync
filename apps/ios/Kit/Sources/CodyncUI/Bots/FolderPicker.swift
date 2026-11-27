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
    @State private var stack: [FolderRoute] = []
    @State private var recents = WorkspacePickerState()
    @State private var confirm = WorkspaceConfirm()
    @State private var saveTask: Task<Void, Never>?
    @State private var selected = ""
    @State private var filter = ""
    /// Opens tall, so Browse other folders shows under a long history; it can be pulled down to half.
    @State private var detent = PresentationDetent.large

    private var online: Bool { model.connection == .online }
    private var history: [RecentWorkspace] { recents.listing?.recent ?? [] }
    private var shown: [RecentWorkspace] { history.filter { filter.isEmpty || $0.path.localizedCaseInsensitiveContains(filter) } }

    var body: some View {
        NavigationStack(path: $stack) {
            recentPage
                .navigationDestination(for: FolderRoute.self) { route in
                    FolderPage(route: route, stack: $stack, online: online, confirm: confirm, use: use)
                }
        }
        .presentationDetents([.medium, .large], selection: $detent)
        // A page coming back into view keeps what it loaded.
        .task(id: model.connection) { await recents.refresh(nil, model: model) }
        .onChange(of: stack) { confirm.error = nil }
        .onDisappear { saveTask?.cancel() }
    }

    private var recentPage: some View {
        List {
            Section {
                LabeledContent {
                    Text(L10n.tr(online ? "Connected" : "Disconnected"))
                } label: {
                    Label(model.hello?.name ?? L10n.tr("Computer"), systemImage: "desktopcomputer").lineLimit(1)
                }
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
            if let error = confirm.error ?? recents.error { Section { ErrorRow(error) } }
            // With no history this is the first thing under the computer.
            Section {
                NavigationLink(value: start.map(FolderRoute.folder) ?? .home) {
                    Label(L10n.tr("Browse other folders"), systemImage: "folder")
                }
            }
        }
        .disabled(confirm.saving)
        .searchField($filter, prompt: L10n.tr("Search recent workspaces"), shown: !history.isEmpty)
        .navigationTitle(L10n.tr("Choose workspace"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button { dismiss() } label: { Image(systemName: "xmark") }.accessibilityLabel(L10n.tr("Cancel"))
            }
            ToolbarItem(placement: .confirmationAction) {
                UseFolderButton(saving: confirm.saving, enabled: !selected.isEmpty && online && !recents.busy) {
                    use(selected, recents.listing)
                }
            }
        }
    }

    private func row(_ item: RecentWorkspace) -> some View {
        Button {
            selected = item.path
            confirm.error = nil
        } label: {
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

    private func use(_ path: String, _ listing: DirListing?) {
        saveTask = Task {
            if let chosen = await confirm.confirm(path, listing: listing, select: model.selectWorkspace, fetch: model.listDirs) {
                onPick(chosen)
            }
        }
    }
}

/// A page of the folder browser: the computer's home, or a folder. On a Windows host the empty path is the drive list,
/// the parent a drive root reports.
enum FolderRoute: Hashable {
    case home
    case folder(String)

    var path: String? {
        if case .folder(let path) = self { path } else { nil }
    }
}

/// The navigation bar's answer, in prominent glass as the sheet's confirming action.
struct UseFolderButton: View {
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

struct LoadingRow: View {
    var body: some View { ProgressView().frame(maxWidth: .infinity) }
}

struct NoteRow: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View { Text(L10n.tr(text)).foregroundStyle(.secondary) }
}

struct ErrorRow: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View { Text(text).foregroundStyle(Palette.danger) }
}

extension View {
    /// The system search field, only while there is something to search.
    @ViewBuilder func searchField(_ text: Binding<String>, prompt: String, shown: Bool = true) -> some View {
        if shown {
            searchable(text: text, prompt: prompt).searchPresentationToolbarBehavior(.avoidHidingContent)
        } else {
            self
        }
    }
}
