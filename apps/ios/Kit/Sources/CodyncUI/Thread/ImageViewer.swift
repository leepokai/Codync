import CodyncKit
import SwiftUI

/// A picture to show full size, with the menu of the message it's in (none in the Library).
struct ImageViewing: Identifiable {
    let source: ImageSource
    var menu: MessageMenu?
    var id: String { source.id }
}

/// Opens a picture in the viewer of the screen showing it (`imageViewer`).
struct OpenImageAction: Sendable {
    let action: @MainActor @Sendable (ImageViewing) -> Void

    @MainActor func callAsFunction(_ viewing: ImageViewing) { action(viewing) }
}

extension EnvironmentValues {
    @Entry var openImage = OpenImageAction { _ in }
}

extension View {
    /// Shows the pictures opened inside this view (`openImage`) full size, in one sheet the screen owns: a chat
    /// row's own sheet would close when its row scrolls away.
    func imageViewer(_ viewing: Binding<ImageViewing?>) -> some View {
        environment(\.openImage, OpenImageAction { viewing.wrappedValue = $0 })
            .codyncSheet(item: viewing) { AttachmentViewer(viewing: $0) }
    }
}

/// A picture full size: pinch, double tap or drag to zoom and move it; the header copies it and, for a file a bot
/// shared, has the file's Download button. Long-press for its message's menu.
struct AttachmentViewer: View {
    let viewing: ImageViewing
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var zoom = ImageZoom.fit
    @State private var zoomReported = false
    @State private var copied: CopyResult?
    @State private var copying: Task<Void, Never>?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader { Text(verbatim: viewing.source.name) } trailing: {
                IconButton(copied?.title ?? "Copy image", systemImage: copied?.symbol ?? "doc.on.doc", action: copyImage)
                if case .shared(let file, let entryId, let botId) = viewing.source {
                    ViewerDownloadButton(file: file, entryId: entryId, botId: botId)
                }
            }
            if case .shared(let file, _, _) = viewing.source { ViewerDownloadStatus(file: file) }
            ZoomableImage(zoom: $zoom) { AttachmentImage(viewing.source, maxPixels: ImageData.largestPixels) }
                .accessibilityLabel(Text(verbatim: viewing.source.name))
                .contextActions(reactions: menu?.reactions) {
                    let copy = ImageCopy.menuItems(viewing.source, from: .viewer, copyImage: copyImage)
                    return menu?.items(copy: copy) ?? copy
                }
                .padding(12)
        }
        .background(Palette.background)
        // Zoomed in, a drag moves the picture instead of closing the sheet.
        .interactiveDismissDisabled(zoom.isZoomed)
        .tracked(.imagePreviewOpened)
        .onChange(of: zoom.scale) {
            guard !zoomReported else { return }
            zoomReported = true
            Analytics.shared.capture(.imagePreviewAction(.zoom, from: .viewer))
        }
        // A download of this chat that finishes while the picture is open shares from here.
        .fileExportSheet(model, botId: viewing.source.botId)
    }

    /// The copy button and the menu's Copy image: copies the picture, then the button shows for a moment whether that
    /// worked.
    private func copyImage() {
        copying?.cancel()
        copying = Task {
            switch await ImageCopy.image(viewing.source, model: model, from: .viewer) {
            case .copied: showCopy(.copied)
            case .failed: showCopy(.failed)
            // A newer copy shows its own result.
            case .overtaken: return
            }
            // Cancelled by a newer copy, which shows its own result.
            try? await Task.sleep(for: CopyResult.shownFor)
            if !Task.isCancelled { showCopy(nil) }
        }
    }

    private func showCopy(_ result: CopyResult?) {
        withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = result }
    }

    /// The message's menu, its reactions as they are now (the sheet keeps the message as it was when opened).
    private var menu: MessageMenu? {
        guard let menu = viewing.menu else { return nil }
        let entry = model.entries[menu.entry.botId]?.first { $0.id == menu.entry.id } ?? menu.entry
        return menu.inViewer(reactions: model.reactionPick(entry)) { dismiss() }
    }
}

/// What the viewer's copy button shows for a moment after a copy.
private enum CopyResult {
    case copied, failed

    static let shownFor = Duration.seconds(1.5)
    var title: String { self == .copied ? "Copied" : "Couldn't copy image" }
    var symbol: String { self == .copied ? "checkmark" : "exclamationmark.circle" }
}

/// A shared file's Download button, beside the viewer's close button: the same states as on its card.
private struct ViewerDownloadButton: View {
    let file: SharedFile
    let entryId: String
    let botId: String
    @Environment(BotStore.self) private var model

    var body: some View {
        let state = model.fileDownloads.states[file.id]
        IconButton(FileDownloadCard.action(state), systemImage: FileDownloadCard.symbol(state)) {
            Motion.animate {
                model.fileDownloads.toggle(client: model.client, entryId: entryId, botId: botId, file: file) { outcome in
                    Analytics.shared.capture(.imagePreviewAction(.download, from: .viewer, outcome: outcome))
                }
            }
        }
    }
}

/// How a shared file's download is going, under the viewer's header (its card is hidden behind the sheet): shown
/// only while it downloads or after it failed.
private struct ViewerDownloadStatus: View {
    let file: SharedFile
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let state = model.fileDownloads.states[file.id]
        let failed: Bool = { if case .failed = state { return true } else { return false } }()
        let shown = failed || model.fileDownloads.isDownloading(file.id)
        Group {
            if shown {
                Text(FileDownloadCard.detail(state, size: file.size))
                    .font(.caption2)
                    .foregroundStyle(failed ? Palette.danger : Palette.secondary)
                    .padding(.horizontal, 20)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .transition(.opacity)
            }
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: shown)
    }
}
