import CodyncKit
import SwiftUI

/// A chat's Library (the details sheet's Library tab): the artifacts its bot published and the files shared in it,
/// grouped by when. A row does what tapping it in the chat does; an artifact's menu also renames or shares it.
struct LibraryView: View {
    let botId: String
    /// Opens an artifact in the chat's viewer (the details sheet closes first).
    let openArtifact: (ArtifactTarget) -> Void
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The files shared in the chat; its artifacts come from the store, where their events keep them current.
    @State private var files: [ChatLibraryItem] = []
    @State private var loaded = false
    @State private var error: String?
    @State private var renaming: Artifact?
    @State private var sharing: ArtifactFile?
    @State private var viewing: Attachment?

    var body: some View {
        let items = files + model.artifacts.artifacts(of: botId).map(ChatLibraryItem.init)
        VStack(alignment: .leading, spacing: 28) {
            if let error {
                Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
            }
            if !items.isEmpty {
                ForEach(ChatLibraryItem.sections(items), id: \.period) { section in
                    CardSection(section.period.rawValue) {
                        ForEach(section.items) { row($0) }
                    }
                }
            } else if loaded {
                empty
            } else {
                Spinner(size: 20).frame(maxWidth: .infinity).padding(.top, 40)
            }
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: loaded)
        .tracked(.screenViewed(.library))
        .task(id: botId) {
            do {
                files = try await model.loadLibrary(botId)
            } catch {
                self.error = error.localizedDescription
            }
            loaded = true
        }
        .codyncSheet(item: $renaming) { ArtifactRenameSheet(artifact: $0) }
        .artifactShareSheet($sharing)
        .codyncSheet(item: $viewing) { AttachmentViewer(file: $0, botId: botId) }
        .fileExportSheet(model, botId: botId)
    }

    @ViewBuilder private func row(_ item: ChatLibraryItem) -> some View {
        switch item.kind {
        case .artifact(let artifact):
            Button { show(artifact) } label: {
                LibraryRow(title: artifact.title) { symbol(ArtifactCard.symbol) }
            }
            .buttonStyle(.plain)
            .contextActions {
                [MenuItem("Open", icon: "arrow.up.forward.square") { show(artifact) },
                 MenuItem("Rename artifact", icon: "pencil") { renaming = artifact },
                 MenuItem("Share artifact", icon: "square.and.arrow.up") { share(artifact) }]
            }
        case .file(let file, let entryId):
            Button { download(file, entryId: entryId) } label: {
                LibraryRow(title: file.name, detail: FileDownloadCard.detail(model.fileDownloads.states[file.id], size: file.size)) {
                    symbol(AttachmentIcon.symbol(file.name))
                }
            }
            .buttonStyle(.plain)
        case .attachment(let file, _) where file.isImage:
            Button { viewing = file } label: {
                LibraryRow(title: file.name) {
                    AttachmentImage(file: file, botId: botId, maxPixels: 120, contentMode: .fill)
                        .frame(width: 36, height: 36)
                        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
            }
            .buttonStyle(.plain)
        case .attachment(let file, _):
            // Like its chat card, nothing to open on the phone: the agent reads it on the computer.
            LibraryRow(title: file.name, detail: ByteCountFormatter.string(fromByteCount: file.size, countStyle: .file)) {
                symbol(AttachmentIcon.symbol(file.name))
            }
        }
    }

    private func symbol(_ name: String) -> some View {
        Image(systemName: name).font(.system(size: 18)).foregroundStyle(Palette.secondary)
    }

    private var empty: some View {
        VStack(spacing: 8) {
            Image(systemName: "books.vertical").font(.system(size: 34)).foregroundStyle(Palette.tertiary)
            Text(L10n.tr("Library is empty")).font(.headline).foregroundStyle(Palette.text)
            Text(L10n.tr("Artifacts {name} makes and files shared in this chat show here.")
                .replacingOccurrences(of: "{name}", with: model.bots[botId]?.name ?? ""))
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
        }
        .multilineTextAlignment(.center)
        .padding(.vertical, 32)
        .frame(maxWidth: .infinity)
    }

    private func show(_ artifact: Artifact) {
        openArtifact(ArtifactTarget(botId: artifact.botId, id: artifact.id, title: artifact.title))
    }

    private func share(_ artifact: Artifact) {
        Task {
            do {
                sharing = ArtifactFile(url: try await model.artifactFile(artifact.id, botId: botId))
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    /// As the file's chat card does: download it for the share sheet, or stop the download under way.
    private func download(_ file: SharedFile, entryId: String) {
        Motion.animate {
            if case .downloading = model.fileDownloads.states[file.id] {
                model.fileDownloads.cancel(file.id)
            } else {
                model.fileDownloads.save(client: model.client, entryId: entryId, botId: botId, file: file)
            }
        }
    }
}

/// One Library row: its picture (a symbol or a thumbnail), the name, and an optional line under it.
private struct LibraryRow<Leading: View>: View {
    let title: String
    var detail: String?
    @ViewBuilder let leading: Leading

    var body: some View {
        HStack(spacing: 12) {
            leading.frame(width: 36, height: 36)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).foregroundStyle(Palette.text).lineLimit(2)
                if let detail { Text(L10n.tr(detail)).font(.caption).foregroundStyle(Palette.secondary) }
            }
            Spacer(minLength: 0)
        }
        .contentShape(Rectangle())
    }
}
