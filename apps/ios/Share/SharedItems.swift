import CodyncKit
import CoreTransferable
import UIKit
import UniformTypeIdentifiers

/// One file another app shared: ready to go, or why it stays behind; its name, and a thumbnail for a picture.
struct IncomingFile: Identifiable {
    enum Content {
        case ready(OutgoingFile)
        /// Over `OutgoingFile.maxSize`.
        case tooLarge
        /// Its app didn't hand it over, or it couldn't be copied or read.
        case unreadable
    }

    let id = UUID()
    let name: String
    let content: Content
    var thumbnail: UIImage?

    var outgoing: OutgoingFile? {
        if case let .ready(file) = content { file } else { nil }
    }

    var isUnreadable: Bool {
        if case .unreadable = content { true } else { false }
    }
}

/// What another app shared: its text and links become the message, everything else a file (`IncomingShare` decides).
/// Files are copied into `folder` (the system hands them over only for the length of a callback) and read from there
/// mapped, so a large one never sits in the extension's memory.
enum SharedItems {
    struct Loaded {
        var text: String
        var files: [IncomingFile]
    }

    @MainActor static func load(_ items: [NSExtensionItem], into folder: URL) async -> Loaded {
        var lines: [String] = []
        var files: [IncomingFile] = []
        for item in items {
            var own: [String] = []
            for provider in item.attachments ?? [] {
                if let line = await line(provider) { own.append(line) }
                else { files.append(await file(provider, folder: folder)) }
            }
            lines += IncomingShare.lines(own, caption: item.attributedContentText?.string)
        }
        return Loaded(text: lines.joined(separator: "\n"), files: files)
    }

    @MainActor private static func line(_ provider: NSItemProvider) async -> String? {
        let types = provider.registeredContentTypes
        if IncomingShare.isLink(types), let url = await load(URL.self, from: provider), !url.isFileURL {
            return url.absoluteString
        }
        guard IncomingShare.isText(types, suggestedName: provider.suggestedName),
              let text = await load(String.self, from: provider)?.trimmingCharacters(in: .whitespacesAndNewlines),
              !text.isEmpty else { return nil }
        return text
    }

    @MainActor private static func load<T: Transferable & Sendable>(_ type: T.Type, from provider: NSItemProvider) async -> T? {
        await withCheckedContinuation { done in
            _ = provider.loadTransferable(type: type) { done.resume(returning: try? $0.get()) }
        }
    }

    @MainActor private static func file(_ provider: NSItemProvider, folder: URL) async -> IncomingFile {
        let suggested = provider.suggestedName
        guard let type = provider.registeredContentTypes.first(where: { $0.conforms(to: .data) }) else {
            return IncomingFile(name: suggested ?? "file", content: .unreadable)
        }
        let copied: URL? = await withCheckedContinuation { done in
            _ = provider.loadFileRepresentation(for: type, openInPlace: false) { url, _, _ in
                done.resume(returning: url.flatMap {
                    copy($0, named: IncomingShare.fileName(suggested, type: type, fallback: $0.lastPathComponent), into: folder)
                })
            }
        }
        guard let copied else { return IncomingFile(name: suggested ?? "file", content: .unreadable) }
        let name = copied.lastPathComponent
        let size = (try? FileManager.default.attributesOfItem(atPath: copied.path())[.size] as? Int) ?? 0
        guard size <= OutgoingFile.maxSize else { return IncomingFile(name: name, content: .tooLarge) }
        guard let data = try? Data(contentsOf: copied, options: .mappedIfSafe) else { return IncomingFile(name: name, content: .unreadable) }
        let file = await Task.detached { OutgoingFile.prepared(name: name, data: data) }.value
        let thumbnail = Attachment.isImage(file.name) ? ImageData.thumbnail(file.data, maxPixels: 120).map(UIImage.init(cgImage:)) : nil
        return IncomingFile(name: file.name, content: .ready(file), thumbnail: thumbnail)
    }

    private static func copy(_ url: URL, named name: String, into folder: URL) -> URL? {
        let plain = name.replacingOccurrences(of: "/", with: "-")
        let target = folder.appending(path: UUID().uuidString).appending(path: plain.isEmpty ? "file" : plain)
        do {
            try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
            try FileManager.default.copyItem(at: url, to: target)
            return target
        } catch {
            return nil
        }
    }
}
