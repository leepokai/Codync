import CodyncKit
import ImageIO
import OSLog
import UIKit
import UniformTypeIdentifiers

private let log = Logger(subsystem: "com.pokai.Codync", category: "Attachments")

/// The picture itself on the clipboard: the file's own bytes as their image type, plus a PNG of them (upright, at most
/// `ImageData.largestPixels` on its long side) when that type isn't one every app pastes.
enum ImageClipboard {
    private static let pastedEverywhere: [UTType] = [.png, .jpeg, .gif]

    /// The clipboard item for `data` (type to bytes); nil when it isn't a picture. Re-encoding a big picture takes a
    /// while: callers keep this off the main actor.
    nonisolated static func item(_ data: Data) -> [String: Data]? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil), CGImageSourceGetCount(source) > 0,
              let identifier = CGImageSourceGetType(source) as String?, let type = UTType(identifier) else { return nil }
        var item: [String: Data] = [type.identifier: data]
        if !pastedEverywhere.contains(type) {
            guard let png = ImageData.png(source, maxPixels: ImageData.largestPixels) else { return nil }
            item[UTType.png.identifier] = png
        }
        return item
    }

    /// Puts `item` on the pasteboard; nil when it worked, else why not.
    @MainActor static func put(_ item: [String: Data]?) -> AnalyticsEvent.ImageError? {
        guard let item else { return .convert }
        UIPasteboard.general.setItems([item])
        // `setItems` reports nothing: the types being there says the write took.
        return UIPasteboard.general.contains(pasteboardTypes: Array(item.keys)) ? nil : .clipboard
    }
}

/// Numbers each copy as it starts, so one that finishes after a newer one began knows it was overtaken.
struct CopyRequests {
    private var latest = 0

    mutating func start() -> Int {
        latest += 1
        return latest
    }

    func isLatest(_ request: Int) -> Bool { request == latest }
}

/// Copying a picture from the chat, the viewer or the Library: the picture itself, or just its name. Each is
/// reported as a viewer action with where it was done and how it went (never the picture or its name); a failed
/// copy is logged.
@MainActor enum ImageCopy {
    /// How a copy of the picture itself went.
    enum Outcome {
        case copied, failed
        /// A newer copy started before this one finished: the clipboard and showing how it went are that one's, and
        /// this one reports nothing.
        case overtaken
    }

    /// Every copy of a picture or its name: whichever started last owns the clipboard.
    static var requests = CopyRequests()

    /// The viewer shows the result on its copy button; from a chat's menu the caller tells the user.
    static func image(_ source: ImageSource, model: BotStore, from: AnalyticsEvent.ImageFrom) async -> Outcome {
        let request = requests.start()
        let data = await source.data(model)
        var item: [String: Data]?
        if let data {
            // The re-encode of a big picture (a TIFF or HEIC to PNG) runs off the main actor; only the pasteboard write is on it.
            item = await Task.detached(priority: .userInitiated) { ImageClipboard.item(data) }.value
        }
        guard requests.isLatest(request) else { return .overtaken }
        let failure: AnalyticsEvent.ImageError? = data == nil ? .unavailable : ImageClipboard.put(item)
        if let failure { log.error("Couldn't copy image: \(failure.rawValue, privacy: .public)") }
        Analytics.shared.capture(.imagePreviewAction(.copyImage, from: from, outcome: failure.map { .failed($0) } ?? .done))
        return failure == nil ? .copied : .failed
    }

    /// The name overtakes a picture copy still converting, so that one can't land over it.
    static func name(_ source: ImageSource, from: AnalyticsEvent.ImageFrom) {
        _ = requests.start()
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
            Task { if await image(source, model: model, from: .menu) == .failed { model.lastError = "Couldn't copy image" } }
        }
    }
}
