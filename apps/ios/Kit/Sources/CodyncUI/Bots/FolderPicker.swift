import CodyncKit
import SwiftUI

struct FolderPicker: View {
    let start: String?
    let onPick: (String) -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var state = WorkspacePickerState()
    @State private var path: String?
    @State private var request = 0
    @State private var pathOpen = false
    @State private var pathInput = ""

    private var noRecent: Bool { state.mode == .recent && !state.busy && (state.listing?.recent ?? []).isEmpty }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Choose workspace")
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("Your bot will work in this folder.").font(.subheadline).foregroundStyle(Palette.secondary)
                    HStack(spacing: 10) {
                        Image(systemName: "desktopcomputer")
                        Text(model.hello?.name ?? "Computer").lineLimit(1)
                        Spacer()
                        Text(model.connection == .online ? "Connected" : "Disconnected")
                            .font(.caption).foregroundStyle(Palette.secondary)
                    }.padding(12).background(Palette.surface, in: RoundedRectangle(cornerRadius: 12))
                    if noRecent { browseButton }
                    if state.mode == .browse { navigation }
                    if pathOpen {
                        HStack {
                            TextField("Paste an absolute path", text: $pathInput)
                                .textFieldStyle(.plain).plainTextInput().onSubmit { openPath() }
                            Button(action: openPath) { Image(systemName: "arrow.right") }
                                .disabled(pathInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                                .accessibilityLabel("Go to folder")
                        }.padding(12).background(Palette.surface, in: RoundedRectangle(cornerRadius: 10))
                            .transition(.opacity.combined(with: .move(edge: .top)))
                    }
                    if !noRecent {
                        SearchField(state.mode == .recent ? "Search recent workspaces" : "Filter this folder", text: $state.filter)
                        Text(state.mode == .recent ? "Recent workspaces" : "Folders")
                            .font(.caption).foregroundStyle(Palette.secondary)
                        if state.busy { Spinner(size: 20).frame(maxWidth: .infinity).padding(24) }
                        else if state.mode == .recent { recentRows }
                        else { directoryRows }
                    }
                    if let error = state.error { Text(error).font(.subheadline).foregroundStyle(Palette.danger) }
                    if state.mode == .recent && !noRecent { browseButton }
                }.padding(.horizontal, 20).padding(.bottom, 20)
                    .disabled(state.saving)
            }.scrollDismissesKeyboard(.interactively)
            footer
        }
        .background(Palette.background)
        .task(id: request) { await state.load(path, model: model) }
        .onChange(of: model.connection) { _, _ in request += 1 }
    }

    private var browseButton: some View {
        Button { open(start) } label: {
            HStack {
                Image(systemName: "folder")
                Text("Browse other folders")
                Spacer()
                Image(systemName: "chevron.right").font(.caption)
            }.padding(.vertical, 12)
        }.buttonStyle(PressScale())
    }

    private var navigation: some View {
        HStack(spacing: 8) {
            Button {
                animate { state.mode = .recent; state.selected = ""; state.filter = ""; path = nil; pathOpen = false; request += 1 }
            } label: { Image(systemName: "chevron.left").frame(width: 32, height: 36) }
                .accessibilityLabel("Recent workspaces")
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    if let ancestors = state.listing?.ancestors {
                        ForEach(ancestors) { item in
                            Button(item.name) { open(item.path) }.font(.caption)
                            if item.id != ancestors.last?.id { Text("/").foregroundStyle(Palette.tertiary) }
                        }
                    } else { Text(state.listing?.path ?? "Folders").font(.caption) }
                }
            }
            Button { if let parent = state.listing?.parent { open(parent) } } label: {
                Image(systemName: "arrow.up").frame(width: 32, height: 36)
            }.disabled(state.listing?.parent == nil || state.busy).accessibilityLabel("Parent folder")
            Button { animate { pathInput = state.listing?.path ?? ""; pathOpen.toggle() } } label: {
                Image(systemName: "pencil").frame(width: 32, height: 36)
            }.accessibilityLabel("Enter folder path")
        }.buttonStyle(PressScale())
    }

    private var recentRows: some View {
        let recent = (state.listing?.recent ?? []).filter { state.filter.isEmpty || $0.path.localizedCaseInsensitiveContains(state.filter) }
        return VStack(spacing: 4) {
            ForEach(recent) { item in
                Button { animate { state.selected = item.path } } label: {
                    HStack(spacing: 12) {
                        Image(systemName: "folder").foregroundStyle(Palette.secondary)
                        VStack(alignment: .leading, spacing: 4) {
                            Text(item.path.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? item.path)
                                .lineLimit(1)
                            Text(item.path).font(.caption).foregroundStyle(Palette.secondary).lineLimit(1).truncationMode(.middle)
                        }
                        Spacer(minLength: 4)
                        if state.selected == item.path { Image(systemName: "checkmark").font(.caption) }
                    }.padding(12).frame(maxWidth: .infinity, alignment: .leading)
                        .background(state.selected == item.path ? Palette.surface : .clear, in: RoundedRectangle(cornerRadius: 12))
                }.buttonStyle(PressScale()).accessibilityAddTraits(state.selected == item.path ? .isSelected : [])
            }
            if recent.isEmpty { empty(state.filter.isEmpty ? "Choose a folder to see it here next time." : "No matching folders.") }
        }
    }

    private var directoryRows: some View {
        let dirs = (state.listing?.dirs ?? []).filter { state.filter.isEmpty || $0.name.localizedCaseInsensitiveContains(state.filter) }
        return VStack(spacing: 0) {
            ForEach(dirs) { dir in
                Button { open(dir.path) } label: {
                    HStack(spacing: 12) {
                        Image(systemName: dir.isGit ? "arrow.triangle.branch" : "folder").foregroundStyle(Palette.secondary)
                        Text(dir.name).lineLimit(1)
                        Spacer()
                        Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Palette.tertiary)
                    }.padding(.vertical, 13).contentShape(Rectangle())
                }.buttonStyle(PressScale())
            }
            if state.listing != nil && dirs.isEmpty { empty(state.filter.isEmpty ? "No subfolders. You can use this folder." : "No matching folders.") }
        }
    }

    private var footer: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(state.mode == .recent ? "Selected folder" : "Current folder").font(.caption2).foregroundStyle(Palette.secondary)
                Text(state.selected.isEmpty ? "Choose a folder" : state.selected).font(.caption).lineLimit(1).truncationMode(.middle)
            }.frame(maxWidth: .infinity, alignment: .leading)
            Button {
                Task { if let chosen = await state.confirm(model: model) { onPick(chosen) } }
            } label: {
                if state.saving { Spinner(size: 16) } else { Text("Use this folder").font(.subheadline) }
            }.buttonStyle(.primary)
                .disabled(state.selected.isEmpty || state.busy || state.saving || model.connection != .online)
        }.padding(20).background(Palette.surface)
    }

    private func empty(_ text: String) -> some View {
        Text(L10n.tr(text)).font(.subheadline).foregroundStyle(Palette.secondary).frame(maxWidth: .infinity).padding(.vertical, 24)
    }
    private func animate(_ action: () -> Void) { withAnimation(Motion.reduced(Motion.layout, reduceMotion), action) }
    private func open(_ next: String?) {
        animate { state.mode = .browse; state.selected = ""; state.filter = ""; pathOpen = false; path = next; request += 1 }
    }
    private func openPath() { let next = pathInput.trimmingCharacters(in: .whitespacesAndNewlines); if !next.isEmpty { open(next) } }
}
