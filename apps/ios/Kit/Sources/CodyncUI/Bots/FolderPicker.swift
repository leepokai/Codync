import CodyncKit
import SwiftUI

/// Browses folders on the host, drilling down in place.
struct FolderPicker: View {
    let start: String?
    let onPick: (String) -> Void
    /// The folders opened so far; the last one is shown.
    @State private var stack: [String?]
    @State private var forward = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    init(start: String?, onPick: @escaping (String) -> Void) {
        self.start = start
        self.onPick = onPick
        _stack = State(initialValue: [start])
    }

    var body: some View {
        let current = stack.last ?? nil
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                if stack.count > 1 {
                    BackButton { go(forward: false) { stack.removeLast() } }
                        .padding(.leading, 12)
                        .transition(.opacity)
                }
                ModalHeader(current.map { ($0 as NSString).lastPathComponent } ?? "Folders")
            }
            ZStack {
                FolderLevel(path: current, onPick: onPick) { path in go(forward: true) { stack.append(path) } }
                    .id(stack.count)
                    .transition(.asymmetric(
                        insertion: .move(edge: forward ? .trailing : .leading),
                        removal: .move(edge: forward ? .leading : .trailing)
                    ).combined(with: .opacity))
            }
            .frame(maxHeight: .infinity, alignment: .top)
            .clipped()
        }
        .background(Palette.background)
    }

    /// Sets the slide direction first, so the leaving level takes it too, then moves.
    private func go(forward: Bool, _ change: @escaping () -> Void) {
        self.forward = forward
        Task { @MainActor in
            withAnimation(Motion.reduced(Motion.layout, reduceMotion), change)
        }
    }
}

/// One folder's subfolders.
private struct FolderLevel: View {
    let path: String?
    let onPick: (String) -> Void
    let open: (String) -> Void
    @Environment(BotStore.self) private var model
    @State private var listing: DirListing?
    @State private var error: String?
    @State private var filter = ""

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 22) {
                SearchField(text: $filter)
                if let listing {
                    CardSection(footer: listing.path) {
                        Button {
                            onPick(listing.path)
                        } label: {
                            Label("Use “\((listing.path as NSString).lastPathComponent)”", systemImage: "checkmark.circle.fill")
                                .font(.headline)
                                .foregroundStyle(Palette.accent)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(PressScale())
                    }
                    let dirs = listing.dirs.filter { filter.isEmpty || $0.name.localizedCaseInsensitiveContains(filter) }
                    if !dirs.isEmpty {
                        CardSection {
                            ForEach(dirs) { dir in
                                Button {
                                    open(dir.path)
                                } label: {
                                    HStack(spacing: 10) {
                                        Image(systemName: dir.isGit ? "arrow.triangle.branch" : "folder")
                                            .foregroundStyle(dir.isGit ? Palette.accent : Palette.secondary)
                                            .frame(width: 20)
                                        Text(dir.name).foregroundStyle(Palette.text).lineLimit(1)
                                        Spacer(minLength: 8)
                                        Image(systemName: "chevron.right")
                                            .font(.caption2.weight(.semibold))
                                            .foregroundStyle(Palette.tertiary)
                                    }
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(PressScale())
                            }
                        }
                    }
                } else if let error {
                    Text(L10n.tr(error)).foregroundStyle(Palette.danger)
                } else {
                    Spinner(size: 20).frame(maxWidth: .infinity)
                }
            }
            .font(.body)
            .padding(20)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Palette.background)
        .task {
            do { listing = try await model.listDirs(path) } catch { self.error = error.localizedDescription }
        }
    }
}
