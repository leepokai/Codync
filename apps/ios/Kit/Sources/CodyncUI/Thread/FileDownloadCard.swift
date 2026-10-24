import CodyncKit
import SwiftUI
import UIKit

struct FileDownloadCard: View {
    let entry: Entry
    let file: SharedFile
    @Environment(BotStore.self) private var model

    var body: some View {
        let state = model.fileDownloads.states[file.id]
        HStack(spacing: 12) {
            Image(systemName: AttachmentIcon.symbol(file.name)).font(.system(size: 24)).foregroundStyle(Palette.secondary)
            VStack(alignment: .leading, spacing: 3) {
                Text(file.name).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(2)
                Text(Self.detail(state, size: file.size)).font(.caption2).foregroundStyle(Palette.secondary)
            }
            Spacer(minLength: 0)
            Button {
                Motion.animate {
                    if case .downloading = state { model.fileDownloads.cancel(file.id) }
                    else { model.fileDownloads.save(client: model.client, entry: entry, file: file) }
                }
            } label: {
                Image(systemName: busy(state) ? "xmark" : "arrow.down.to.line")
                    .font(.system(size: 20)).frame(width: 36, height: 36)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(busy(state) ? "Cancel download" : "Download") \(file.name)")
        }
        .padding(14)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 16))
    }

    private func busy(_ state: FileDownloads.State?) -> Bool {
        if case .downloading = state { return true }
        return false
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
