import ActivityKit
import CryptoKit
import Foundation
import UIKit

/// Static PNGs shared by this account’s Live Activities. No network reads.
public enum ActivityIconCache {
    public static func writeImage(_ icon: CustomIcon, context: String) throws {
        guard let data = icon.pngData, let filename = icon.filename, let directory = directory(context) else { return }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let file = directory.appendingPathComponent(filename)
        if !FileManager.default.fileExists(atPath: file.path) { try data.write(to: file, options: .atomic) }
    }

    public static func erase(context: String) throws {
        guard let directory = directory(context), FileManager.default.fileExists(atPath: directory.path) else { return }
        try FileManager.default.removeItem(at: directory)
    }

    public struct ActivityReference: Codable, Sendable {
        public var name: String
        public var tinted: Bool
    }

    /// A small local pointer lets an already-running activity follow replacements/removals.
    public static func writeReference(_ icon: CustomIcon?, computerId: ComputerID, botId: String, context: String) throws {
        try icon?.cacheForActivity(context: context)
        guard let directory = directory(context), let file = referenceURL(computerId, botId, context) else { return }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let reference = ActivityReference(name: icon?.filename ?? "", tinted: icon?.tinted ?? false)
        try JSONEncoder().encode(reference).write(to: file, options: .atomic)
    }

    public static func activityReference(computerId: ComputerID, botId: String, context: String) -> ActivityReference? {
        guard let file = referenceURL(computerId, botId, context), let data = try? Data(contentsOf: file) else { return nil }
        return try? JSONDecoder().decode(ActivityReference.self, from: data)
    }

    /// Remove superseded PNGs, preserving references belonging to other computers and activities.
    public static func prune(context: String) throws {
        guard let directory = directory(context), FileManager.default.fileExists(atPath: directory.path) else { return }
        let files = try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
        var retained = Set(Activity<BotActivityAttributes>.activities.filter { $0.attributes.iconContext == context }.compactMap { $0.attributes.currentCustomIconName })
        for file in files where file.lastPathComponent.hasPrefix("activity-") && file.pathExtension == "json" {
            let reference = try JSONDecoder().decode(ActivityReference.self, from: Data(contentsOf: file))
            if !reference.name.isEmpty { retained.insert(reference.name) }
        }
        for file in files where file.pathExtension == "png" && !retained.contains(file.lastPathComponent) {
            try FileManager.default.removeItem(at: file)
        }
    }

    private static func referenceURL(_ computerId: ComputerID, _ botId: String, _ context: String) -> URL? {
        let key = SHA256.hash(data: Data("\(computerId)\u{0}\(botId)".utf8)).map { String(format: "%02x", $0) }.joined()
        return directory(context)?.appendingPathComponent("activity-\(key).json")
    }

    @MainActor public static func image(filename: String, context: String) -> UIImage? {
        guard filename.count == 68, filename.hasSuffix(".png"),
              filename.dropLast(4).allSatisfy({ $0.isHexDigit }), let directory = directory(context) else { return nil }
        return UIImage(contentsOfFile: directory.appendingPathComponent(filename).path)
    }

    private static func directory(_ context: String) -> URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: SharedStore.appGroup)?
            .appendingPathComponent("CustomIcons", isDirectory: true)
            .appendingPathComponent(SHA256.hash(data: Data(context.utf8)).map { String(format: "%02x", $0) }.joined(), isDirectory: true)
    }
}
