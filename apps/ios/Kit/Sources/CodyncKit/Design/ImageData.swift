import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Decoding, downscaling and re-encoding pictures without holding more pixels than the phone can.
public enum ImageData {
    /// The long side, in pixels, of the largest picture the app decodes (the viewer, a converted upload, the clipboard's
    /// PNG): a bigger one is scaled down to it, so a small compressed file can't take more memory than the phone has.
    public static let largestPixels = 4096

    public static func isImage(_ data: Data) -> Bool {
        CGImageSourceCreateWithData(data as CFData, nil).map { CGImageSourceGetCount($0) > 0 } ?? false
    }

    public static func isPNG(_ data: Data) -> Bool { data.starts(with: [0x89, 0x50, 0x4E, 0x47]) }

    /// A downscaled, upright image (`maxPixels` on the long side).
    public static func thumbnail(_ data: Data, maxPixels: Int) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixels,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }

    public static func jpeg(_ data: Data) -> Data? {
        guard let image = thumbnail(data, maxPixels: largestPixels) else { return nil }
        return encode(image, as: .jpeg, [kCGImageDestinationLossyCompressionQuality: 0.85])
    }

    /// The picture's first frame, upright (its orientation applied), as PNG: full size up to `maxPixels` on the long
    /// side, scaled down to it past that.
    public static func png(_ source: CGImageSource, maxPixels: Int) -> Data? {
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
