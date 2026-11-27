import CodyncKit
import SwiftUI

/// One folder: its subfolders, each pushing its own page, filtered by the search field. The title's menu goes to the
/// parent or an ancestor, or opens a field for a pasted path; Use this folder takes the folder shown.
struct FolderPage: View {
    let route: FolderRoute
    @Binding var stack: [FolderRoute]
    let online: Bool
    let confirm: WorkspaceConfirm
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
        let name = (folder.listing?.path ?? route.path ?? "").folderName
        return name.isEmpty ? L10n.tr("Folders") : name
    }
    /// The drive list (an empty path) is no folder to work in.
    private var usable: Bool { !(folder.listing?.path.isEmpty ?? true) }

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
                        NavigationLink(value: FolderRoute.folder(dir.path)) {
                            Label(dir.name, systemImage: dir.isGit ? "arrow.triangle.branch" : "folder")
                        }
                    }
                    if dirs.isEmpty { NoteRow(filter.isEmpty ? "No subfolders. You can use this folder." : "No matching folders.") }
                } header: {
                    Text(listing.path).textCase(nil).lineLimit(1).truncationMode(.middle)
                }
            }
            if let error = confirm.error ?? folder.error { Section { ErrorRow(error) } }
        }
        .disabled(confirm.saving)
        .searchField($filter, prompt: L10n.tr("Filter this folder"))
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarTitleMenu { titleMenu }
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                UseFolderButton(saving: confirm.saving, enabled: usable && online && !folder.busy) {
                    if let listing = folder.listing { use(listing.path, listing) }
                }
            }
        }
        .task(id: model.connection) { await folder.refresh(route.path, model: model) }
    }

    @ViewBuilder private var titleMenu: some View {
        Button(L10n.tr("Enter folder path"), systemImage: "pencil") {
            typed = folder.listing?.path ?? route.path ?? ""
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
    private func open(_ path: String) {
        let next = FolderRoute.folder(path)
        if let index = stack.firstIndex(of: next) { stack = Array(stack.prefix(through: index)) } else { stack.append(next) }
    }

    private func go() {
        let next = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !next.isEmpty else { return }
        typing = false
        open(next)
    }
}
