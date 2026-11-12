import CodyncKit
import ImageIO
import SwiftUI
import UniformTypeIdentifiers

/// Files in messages: preparing picked, pasted and dropped files, and showing sent ones.

extension OutgoingFile {
    /// Images the agents can't all read (HEIC, TIFF, BMP…) become JPEG, upright.
    static func prepared(name: String, data: Data) -> OutgoingFile {
        let ext = (name as NSString).pathExtension.lowercased()
        guard ["heic", "heif", "tif", "tiff", "bmp"].contains(ext) || (ext.isEmpty && ImageData.isImage(data)),
              let jpeg = ImageData.jpeg(data) else { return OutgoingFile(name: name, data: data) }
        let base = (name as NSString).deletingPathExtension
        return OutgoingFile(name: (base.isEmpty ? "image" : base) + ".jpg", data: jpeg)
    }

    /// A name for a file that came without one (pasted or dragged image data).
    static func unnamed(_ ext: String) -> String {
        "pasted-\(Int(Date.now.timeIntervalSince1970)).\(ext)"
    }
}

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

enum ImageData {
    /// The long side, in pixels, of the largest picture the app decodes (the viewer, a converted upload, the clipboard's
    /// PNG): a bigger one is scaled down to it, so a small compressed file can't take more memory than the phone has.
    static let largestPixels = 4096

    static func isImage(_ data: Data) -> Bool {
        CGImageSourceCreateWithData(data as CFData, nil).map { CGImageSourceGetCount($0) > 0 } ?? false
    }

    static func isPNG(_ data: Data) -> Bool { data.starts(with: [0x89, 0x50, 0x4E, 0x47]) }

    /// A downscaled, upright image (`maxPixels` on the long side).
    static func thumbnail(_ data: Data, maxPixels: Int) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixels,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }

    static func jpeg(_ data: Data) -> Data? {
        guard let image = thumbnail(data, maxPixels: largestPixels) else { return nil }
        return encode(image, as: .jpeg, [kCGImageDestinationLossyCompressionQuality: 0.85])
    }

    /// The picture's first frame, upright (its orientation applied), as PNG: full size up to `maxPixels` on the long
    /// side, scaled down to it past that.
    static func png(_ source: CGImageSource, maxPixels: Int) -> Data? {
        let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
        let width = properties?[kCGImagePropertyPixelWidth] as? Int ?? 0
        let height = properties?[kCGImagePropertyPixelHeight] as? Int ?? 0
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: min(max(width, height, 1), maxPixels),
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary).flatMap { encode($0, as: .png) }
    }

    private static func encode(_ image: CGImage, as type: UTType, _ options: [CFString: Any] = [:]) -> Data? {
        let out = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(out, type.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(dest, image, options as CFDictionary)
        return CGImageDestinationFinalize(dest) ? out as Data : nil
    }
}

/// The symbol for a file by its extension.
enum AttachmentIcon {
    static func symbol(_ name: String) -> String {
        if Attachment.isImage(name) { return "photo" }
        if Attachment.isAudio(name) { return "waveform" }
        return switch (name as NSString).pathExtension.lowercased() {
        case "pdf": "doc.richtext"
        case "zip", "gz", "tar", "7z": "doc.zipper"
        case "mov", "mp4", "m4v": "film"
        default: "doc"
        }
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
