import CodyncKit
import SwiftUI
import UIKit

/// A file a bot shared: its picture (tap for full size) when it's an image, its name and size, and its Download button.
/// Long-press for the message's menu; a picture's has Copy image and Copy name for Copy.
struct FileDownloadCard: View {
    let entry: Entry
    let file: SharedFile
    let menu: MessageMenu
    @Environment(BotStore.self) private var model
    @Environment(\.openImage) private var openImage

    private var source: ImageSource { .shared(file, entryId: entry.id, botId: entry.botId) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if file.showsAsImage { preview }
            row
        }
        .padding(14)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 16))
        .contextActions(reactions: menu.reactions) {
            menu.items(copy: file.showsAsImage ? ImageCopy.menuItems(source, model: model) : menu.textCopy)
        }
    }

    /// The picture itself; tap for full size.
    private var preview: some View {
        AttachmentImage(source, maxPixels: 720)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .contentShape(Rectangle())
            .onTapGesture { openImage(ImageViewing(source: source, menu: menu)) }
            .accessibilityLabel(file.name)
            .accessibilityAddTraits(.isButton)
            .frame(maxWidth: .infinity, maxHeight: 280, alignment: .leading)
    }

    private var row: some View {
        let state = model.fileDownloads.states[file.id]
        return HStack(spacing: 12) {
            Image(systemName: AttachmentIcon.symbol(file.name)).font(.system(size: 24)).foregroundStyle(Palette.secondary)
            VStack(alignment: .leading, spacing: 3) {
                Text(file.name).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(2)
                Text(Self.detail(state, size: file.size)).font(.caption2).foregroundStyle(Palette.secondary)
            }
            Spacer(minLength: 0)
            Button {
                Motion.animate { model.fileDownloads.toggle(client: model.client, entryId: entry.id, botId: entry.botId, file: file) }
            } label: {
                Image(systemName: Self.symbol(state))
                    .font(.system(size: 20)).frame(width: 36, height: 36)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text(verbatim: L10n.tr(Self.action(state)) + " " + file.name))
        }
    }

    /// What the Download button does now (its label).
    static func action(_ state: FileDownloads.State?) -> String {
        switch state {
        case .downloading: "Cancel download"
        case .failed: "Retry download"
        case .saved: "Save again"
        case nil: "Download"
        }
    }

    static func symbol(_ state: FileDownloads.State?) -> String {
        if case .downloading = state { return "xmark" }
        return "arrow.down.to.line"
    }

    /// The size, or how the download is going.
    static func detail(_ state: FileDownloads.State?, size bytes: Int64) -> String {
        let size = ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
        switch state {
        case .downloading(let received): return "\(ByteCountFormatter.string(fromByteCount: received, countStyle: .file)) of \(size)"
        case .failed(let message): return "\(message) Tap to retry."
        case .saved: return "\(size) · Downloaded"
        case nil: return size
        }
    }
}

extension View {
    /// The share sheet for a file of this chat once it's downloaded (`FileDownloads.export`). Off (`active`) while
    /// a sheet over this view presents it instead.
    func fileExportSheet(_ model: BotStore, botId: String, active: Bool = true) -> some View {
        codyncSheet(item: Binding<FileDownloads.Export?>(get: {
            guard active, let item = model.fileDownloads.export, item.botId == botId else { return nil }
            return item
        }, set: { if $0 == nil { model.fileDownloads.dismissExport() } })) { item in
            FileExportSheet(url: item.url) { model.fileDownloads.dismissExport() }
        }
    }
}

/// iOS owns save/export destinations through its native share sheet.
struct FileExportSheet: UIViewControllerRepresentable {
    let url: URL
    let completed: () -> Void
    func makeUIViewController(context: Context) -> UIActivityViewController {
        let controller = UIActivityViewController(activityItems: [url], applicationActivities: nil)
        controller.completionWithItemsHandler = { _, _, _, _ in completed() }
        return controller
    }
    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}
