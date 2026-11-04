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

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader { Text(verbatim: viewing.source.name) } trailing: {
                CopyImageButton(source: viewing.source)
                if case .shared(let file, let entryId, let botId) = viewing.source {
                    ViewerDownloadButton(file: file, entryId: entryId, botId: botId)
                }
            }
            ZoomableImage(zoom: $zoom) { AttachmentImage(viewing.source, maxPixels: 4096) }
                .accessibilityLabel(Text(verbatim: viewing.source.name))
                .contextActions(reactions: menu?.reactions) {
                    let copy = ImageCopy.menuItems(viewing.source, model: model)
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
            Analytics.shared.capture(.imagePreviewAction(.zoom))
        }
        // A download of this chat that finishes while the picture is open shares from here.
        .fileExportSheet(model, botId: viewing.source.botId)
    }

    /// The message's menu, its reactions as they are now (the sheet keeps the message as it was when opened).
    private var menu: MessageMenu? {
        guard let menu = viewing.menu else { return nil }
        let entry = model.entries[menu.entry.botId]?.first { $0.id == menu.entry.id } ?? menu.entry
        return menu.inViewer(reactions: model.reactionPick(entry)) { dismiss() }
    }
}

/// The viewer's copy button: copies the picture, then shows for a moment whether that worked.
private struct CopyImageButton: View {
    let source: ImageSource
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var result: Result?
    @State private var copying: Task<Void, Never>?

    enum Result {
        case copied, failed

        static let shownFor = Duration.seconds(1.5)
        var title: String { self == .copied ? "Copied" : "Couldn't copy image" }
        var symbol: String { self == .copied ? "checkmark" : "exclamationmark.circle" }
    }

    var body: some View {
        IconButton(result?.title ?? "Copy image", systemImage: result?.symbol ?? "doc.on.doc") {
            copying?.cancel()
            copying = Task {
                let copied = await ImageCopy.image(source, model: model)
                show(copied ? .copied : .failed)
                // Cancelled by a newer copy, which shows its own result.
                try? await Task.sleep(for: Result.shownFor)
                if !Task.isCancelled { show(nil) }
            }
        }
    }

    private func show(_ result: Result?) {
        withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { self.result = result }
    }
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
            if !model.fileDownloads.isDownloading(file.id) { Analytics.shared.capture(.imagePreviewAction(.download)) }
            Motion.animate { model.fileDownloads.toggle(client: model.client, entryId: entryId, botId: botId, file: file) }
        }
    }
}
