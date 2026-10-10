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
                Text(detail(state)).font(.caption2).foregroundStyle(Palette.secondary)
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
    private func detail(_ state: FileDownloads.State?) -> String {
        let size = ByteCountFormatter.string(fromByteCount: file.size, countStyle: .file)
        switch state {
        case .downloading(let received): return "\(ByteCountFormatter.string(fromByteCount: received, countStyle: .file)) of \(size)"
        case .failed(let message): return "\(message) Tap to retry."
        case .saved: return "\(size) · Downloaded"
        case nil: return size
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
