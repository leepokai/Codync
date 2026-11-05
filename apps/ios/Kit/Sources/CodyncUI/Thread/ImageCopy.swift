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

    /// nil when it worked, else why not.
    @MainActor static func copy(_ data: Data) -> AnalyticsEvent.ImageError? {
        guard let item = item(data) else { return .convert }
        UIPasteboard.general.setItems([item])
        // `setItems` reports nothing: the types being there says the write took.
        return UIPasteboard.general.contains(pasteboardTypes: Array(item.keys)) ? nil : .clipboard
    }
}

/// Copying a picture from the chat, the viewer or the Library: the picture itself, or just its name. Each is
/// reported as a viewer action with where it was done and how it went (never the picture or its name); a failed
/// copy is logged.
@MainActor enum ImageCopy {
    /// Whether the picture reached the clipboard. The viewer shows the result on its copy button; from a chat's menu
    /// the caller tells the user.
    @discardableResult
    static func image(_ source: ImageSource, model: BotStore, from: AnalyticsEvent.ImageFrom) async -> Bool {
        let failure: AnalyticsEvent.ImageError?
        if let data = await source.data(model) {
            failure = ImageClipboard.copy(data)
        } else {
            failure = .unavailable
        }
        if let failure { log.error("Couldn't copy image: \(failure.rawValue, privacy: .public)") }
        Analytics.shared.capture(.imagePreviewAction(.copyImage, from: from, outcome: failure.map { .failed($0) } ?? .done))
        return failure == nil
    }

    static func name(_ source: ImageSource, from: AnalyticsEvent.ImageFrom) {
        Pasteboard.copy(source.name)
        Analytics.shared.capture(.imagePreviewAction(.copyName, from: from))
    }

    /// What takes a message's Copy place when it holds this picture; `copyImage` copies it (the viewer's shows the
    /// result on its copy button).
    static func menuItems(_ source: ImageSource, from: AnalyticsEvent.ImageFrom, copyImage: @escaping () -> Void) -> [MenuItem] {
        [MenuItem("Copy image", icon: "photo.on.rectangle", action: copyImage),
         MenuItem("Copy name", icon: "square.on.square") { name(source, from: from) }]
    }

    /// The items in the chat, where nothing shows the result: a failed copy says so in the app's error dialog.
    static func menuItems(_ source: ImageSource, model: BotStore) -> [MenuItem] {
        menuItems(source, from: .menu) {
            Task { if !(await image(source, model: model, from: .menu)) { model.lastError = "Couldn't copy image" } }
        }
    }
}
