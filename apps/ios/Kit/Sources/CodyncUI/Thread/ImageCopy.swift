import CodyncKit
import ImageIO
import UIKit
import UniformTypeIdentifiers

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
/// recorded as a viewer action (never the picture or its name).
@MainActor enum ImageCopy {
    /// Whether the picture reached the clipboard.
    static func image(_ source: ImageSource, model: BotStore) async -> Bool {
        guard let data = await source.data(model), ImageClipboard.copy(data) else { return false }
        Analytics.shared.capture(.imagePreviewAction(.copyImage))
        return true
    }

    static func name(_ source: ImageSource) {
        Pasteboard.copy(source.name)
        Analytics.shared.capture(.imagePreviewAction(.copyName))
    }

    /// What takes a message's Copy place when it holds this picture. A copy from a menu has nowhere to show a
    /// failure; the viewer's copy button shows it.
    static func menuItems(_ source: ImageSource, model: BotStore) -> [MenuItem] {
        [MenuItem("Copy image", icon: "photo.on.rectangle") { Task { _ = await image(source, model: model) } },
         MenuItem("Copy name", icon: "square.on.square") { name(source) }]
    }
}
