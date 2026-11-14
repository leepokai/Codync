import CodyncKit
import ImageIO
import SwiftUI
import UniformTypeIdentifiers

/// Files in messages: preparing picked, pasted and dropped files, and showing sent ones.

/// Anything dropped or pasted: a file, or bare image data.
struct PickedFile: Transferable {
    let file: OutgoingFile

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .item) { received in
            PickedFile(file: .prepared(name: received.file.lastPathComponent, data: try Data(contentsOf: received.file)))
        }
        DataRepresentation(importedContentType: .image) { data in
            PickedFile(file: .prepared(name: OutgoingFile.unnamed(ImageData.isPNG(data) ? "png" : "jpg"), data: data))
        }
    }

    /// Loads what item providers (pasteboard, drops) carry.
    @MainActor static func load(_ providers: [NSItemProvider]) async -> [OutgoingFile] {
        var files: [OutgoingFile] = []
        for provider in providers {
            let file: OutgoingFile? = await withCheckedContinuation { done in
                _ = provider.loadTransferable(type: PickedFile.self) { done.resume(returning: try? $0.get().file) }
            }
            if let file { files.append(file) }
        }
        return files
    }
}

/// Where a picture's bytes are on the computer: a file sent with a message, or one a bot shared.
enum ImageSource: Hashable {
    case sent(Attachment, botId: String)
    case shared(SharedFile, entryId: String, botId: String)

    var id: String {
        switch self {
        case .sent(let file, _): file.id
        case .shared(let file, _, _): file.id
        }
    }

    var name: String {
        switch self {
        case .sent(let file, _): file.name
        case .shared(let file, _, _): file.name
        }
    }

    /// The chat it's in.
    var botId: String {
        switch self {
        case .sent(_, let botId), .shared(_, _, let botId): botId
        }
    }

    /// The whole file, fetched once and then read from the cache.
    @MainActor func data(_ model: BotStore) async -> Data? {
        switch self {
        case .sent(let file, let botId): await model.attachmentData(file, bot: botId)
        case .shared(let file, let entryId, _): await model.sharedFileData(file, entryId: entryId)
        }
    }
}

/// Files sent with a message, on the sender's side: images as pictures (tap for full size; long-press for the
/// message's menu, with Copy image and Copy name for Copy), other files as cards.
struct AttachmentList: View {
    let attachments: [Attachment]
    let botId: String
    let menu: MessageMenu
    @Environment(BotStore.self) private var model
    @Environment(\.openImage) private var openImage

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            ForEach(attachments) { file in
                if file.isImage {
                    let source = ImageSource.sent(file, botId: botId)
                    AttachmentImage(source, maxPixels: 720)
                        .frame(maxWidth: 220, maxHeight: 280)
                        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                        .contentShape(Rectangle())
                        .onTapGesture { openImage(ImageViewing(source: source, menu: menu)) }
                        .contextActions(reactions: menu.reactions) { menu.items(copy: ImageCopy.menuItems(source, model: model)) }
                        .accessibilityLabel(file.name)
                        .accessibilityAddTraits(.isButton)
                } else {
                    card(file)
                }
            }
        }
    }

    private func card(_ file: Attachment) -> some View {
        HStack(spacing: 8) {
            Image(systemName: AttachmentIcon.symbol(file.name))
                .font(.system(size: 16))
                .foregroundStyle(Palette.secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text(file.name)
                    .font(.subheadline)
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(ByteCountFormatter.string(fromByteCount: file.size, countStyle: .file))
                    .font(.caption2)
                    .foregroundStyle(Palette.tertiary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

/// A picture from the computer (a sent image or one a bot shared), fetched once and then read from the cache.
struct AttachmentImage: View {
    let source: ImageSource
    let maxPixels: Int
    var contentMode: ContentMode = .fit
    @Environment(BotStore.self) private var model
    @State private var image: CGImage?

    init(_ source: ImageSource, maxPixels: Int, contentMode: ContentMode = .fit) {
        self.source = source
        self.maxPixels = maxPixels
        self.contentMode = contentMode
    }

    var body: some View {
        Group {
            if let image {
                Image(decorative: image, scale: 1).resizable().aspectRatio(contentMode: contentMode)
            } else {
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(Palette.bubbleUser)
                    .frame(width: 160, height: 120)
                    .overlay { Image(systemName: "photo").foregroundStyle(Palette.tertiary) }
            }
        }
        // Asked again when the computer comes online: a chat shown from the cache before it connects
        // gets its pictures once it does. A loaded picture is never fetched again.
        .task(id: [source.id, model.connection == .online ? "online" : "waiting"]) {
            guard image == nil, let data = await source.data(model) else { return }
            let size = maxPixels
            image = await Task.detached { ImageData.thumbnail(data, maxPixels: size) }.value
        }
    }
}
