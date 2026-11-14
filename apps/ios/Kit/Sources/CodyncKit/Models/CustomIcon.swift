import CryptoKit
import Foundation
import UIKit

/// The host supplies a static, normalized PNG, never an arbitrary remote URL.
public struct CustomIcon: Codable, Hashable, Sendable {
    public var dataUrl: String
    public var tinted: Bool

    public var pngData: Data? {
        let prefix = "data:image/png;base64,"
        guard dataUrl.hasPrefix(prefix), dataUrl.utf8.count <= 65_558,
              let data = Data(base64Encoded: String(dataUrl.dropFirst(prefix.count))),
              data.starts(with: [137, 80, 78, 71, 13, 10, 26, 10]) else { return nil }
        return data
    }

    /// Only the small filename goes in ActivityKit's limited attributes, not the image bytes.
    public var filename: String? {
        pngData.map { SHA256.hash(data: $0).map { String(format: "%02x", $0) }.joined() + ".png" }
    }

    @MainActor private static let decoded: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>()
        cache.countLimit = 128
        cache.totalCostLimit = 8 * 1024 * 1024
        return cache
    }()

    @MainActor public var image: UIImage? {
        guard let data = pngData, let filename else { return nil }
        if let cached = Self.decoded.object(forKey: filename as NSString) { return cached }
        guard let image = UIImage(data: data) else { return nil }
        Self.decoded.setObject(image, forKey: filename as NSString, cost: Int(image.size.width * image.size.height) * 4)
        return image
    }

    public func cacheForActivity(context: String) throws {
        try ActivityIconCache.writeImage(self, context: context)
    }
}
