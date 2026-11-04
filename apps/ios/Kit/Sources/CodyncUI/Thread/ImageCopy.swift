import CodyncKit
import ImageIO
import OSLog
import UIKit
import UniformTypeIdentifiers

private let log = Logger(subsystem: "com.pokai.Codync", category: "Attachments")

/// The picture itself on the clipboard: the file's own bytes as their image type, plus a PNG of them when that type
/// isn't one every app pastes.
enum ImageClipboard {
    private static let pastedEverywhere: [UTType] = [.png, .jpeg, .gif]

    /// The clipboard item for `data`; nil when it isn't a picture.
    static func item(_ data: Data) -> [String: Any]? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil), CGImageSourceGetCount(source) > 0,
              let identifier = CGImageSourceGetType(source) as String?, let type = UTType(identifier) else { return nil }
        var item: [String: Any] = [type.identifier: data]
        if !pastedEverywhere.contains(type) {
            guard let png = ImageData.png(source) else { return nil }
            item[UTType.png.identifier] = png
        }
        return item
    }

    /// Whether it worked.
    @MainActor static func copy(_ data: Data) -> Bool {
        guard let item = item(data) else { return false }
        UIPasteboard.general.setItems([item])
        return true
    }
}

/// Copying a picture from the chat, the viewer or the Library: the picture itself, or just its name. Each is
/// recorded as a viewer action (never the picture or its name); a failed copy is logged.
@MainActor enum ImageCopy {
    /// Whether the picture reached the clipboard. The viewer shows the result; a chat menu has only the log.
    @discardableResult
    static func image(_ source: ImageSource, model: BotStore) async -> Bool {
        guard let data = await source.data(model) else {
            log.error("Couldn't copy image: the picture couldn't be read")
            return false
        }
        guard ImageClipboard.copy(data) else {
            log.error("Couldn't copy image: the file isn't a picture the clipboard takes")
            return false
        }
        Analytics.shared.capture(.imagePreviewAction(.copyImage))
        return true
    }

    static func name(_ source: ImageSource) {
        Pasteboard.copy(source.name)
        Analytics.shared.capture(.imagePreviewAction(.copyName))
    }

    /// What takes a message's Copy place when it holds this picture; `copyImage` copies it (the viewer's shows the
    /// result on its copy button).
    static func menuItems(_ source: ImageSource, copyImage: @escaping () -> Void) -> [MenuItem] {
        [MenuItem("Copy image", icon: "photo.on.rectangle", action: copyImage),
         MenuItem("Copy name", icon: "square.on.square") { name(source) }]
    }

    /// The items in the chat, where nothing shows the result.
    static func menuItems(_ source: ImageSource, model: BotStore) -> [MenuItem] {
        menuItems(source) { Task { await image(source, model: model) } }
    }
}
